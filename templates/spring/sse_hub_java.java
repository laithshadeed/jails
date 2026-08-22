package {{pkg}};

import java.io.IOException;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;
import org.springframework.web.servlet.mvc.method.annotation.SseEmitter;

/**
 * Server-Sent Events: one place that owns every open connection.
 *
 * <p>Four details here are the ones this design gets wrong when it is written
 * from memory, and each is a silent failure rather than a compile error.
 *
 * <ol>
 *   <li><b>The timeout is {@code -1L}, not {@code Long.MAX_VALUE}.</b> It
 *       reaches {@code AsyncContext.setTimeout}, where the Servlet spec reads
 *       zero or less as "no timeout". {@code Long.MAX_VALUE} is a real timeout
 *       that containers are free to reject or overflow.
 *   <li><b>{@code onCompletion} alone is enough to remove an emitter</b> --
 *       Spring calls it when the request completes "for any reason including
 *       timeout and network error". But it runs on a <i>container</i> thread,
 *       concurrently with whatever is broadcasting, which is why the registry
 *       is a {@link ConcurrentHashMap} of {@link ConcurrentHashMap#newKeySet}
 *       and not a {@code HashMap} of {@code HashSet}.
 *   <li><b>The heartbeat must not run on the default scheduler.</b>
 *       {@code spring.task.scheduling.pool.size} defaults to <b>1</b>, so one
 *       heartbeat blocking on one dead client stalls every other scheduled job
 *       in the application. The property this capability writes raises it; if
 *       you take that property out, take the {@code @Scheduled} out with it.
 *   <li><b>No event {@code id()} is emitted.</b> Spring does not implement
 *       {@code Last-Event-ID} -- there is no replay path in the framework, and
 *       none here. Emitting an id would advertise resumability this does not
 *       have, and a browser would reconnect expecting to be caught up.
 * </ol>
 *
 * <p>Framework 7 replaced {@code synchronized} with a {@code ReentrantLock}
 * throughout {@code ResponseBodyEmitter}, so sending from a virtual thread does
 * not pin its carrier. That is what makes one thread per subscriber viable.
 */
@Component
public class {{name}}Hub {

    /** No timeout: the client, not the container, decides when to leave. */
    private static final long NO_TIMEOUT = -1L;

    private final Map<String, Set<SseEmitter>> subscribers = new ConcurrentHashMap<>();

    /**
     * Open a stream for one topic.
     *
     * @param topic what the caller is subscribing to -- a resource id, a user,
     *     a channel. Keys are the caller's; this class never invents one.
     */
    public SseEmitter subscribe(String topic) {
        SseEmitter emitter = new SseEmitter(NO_TIMEOUT);
        // The add happens *inside* `compute`, not after `computeIfAbsent`
        // returns. Outside, a concurrent removal that finds the set empty can
        // drop it from the map between the lookup and the add, and this
        // subscriber is then live but unreachable -- it receives nothing and
        // nothing ever completes it. Both sides take the same bin lock.
        subscribers.compute(
                topic,
                (key, existing) -> {
                    Set<SseEmitter> forTopic =
                            existing == null ? ConcurrentHashMap.newKeySet() : existing;
                    forTopic.add(emitter);
                    return forTopic;
                });

        // One callback, not three. `onCompletion` fires for a clean close, a
        // timeout and a broken connection alike; adding `onTimeout` and
        // `onError` beside it removes the same emitter two or three times and
        // reads as if they covered different cases.
        //
        // It fires only once the emitter is *bound to a request*, though:
        // `ResponseBodyEmitter.complete()` sets a flag and forwards to a
        // handler that does not exist until Spring MVC returns the emitter to
        // the container. So this is the production path and {@link
        // #unsubscribe} is the one anything else -- a caller that already
        // knows the client is gone, or a test -- has to use.
        emitter.onCompletion(() -> unsubscribe(topic, emitter));
        return emitter;
    }

    /** Send to everyone on a topic. Dead connections are dropped as found. */
    public void publish(String topic, String event, Object payload) {
        for (SseEmitter emitter : subscribers.getOrDefault(topic, Set.of())) {
            try {
                emitter.send(SseEmitter.event().name(event).data(payload));
            } catch (IOException | IllegalStateException gone) {
                // The client vanished between the iteration and the write.
                // `completeWithError` triggers `onCompletion`, which is what
                // takes it out of the map -- doing it here as well would be a
                // second removal path to keep in step with the first.
                emitter.completeWithError(gone);
            }
        }
    }

    /**
     * Drop one connection now, without waiting for a write to fail.
     *
     * <p>Public because it is genuinely needed: {@code onCompletion} runs only
     * for an emitter the container is holding, so anything that learns the
     * client has gone by another route -- a logout, a deleted resource, a test
     * -- has no other way to say so. Idempotent.
     */
    public void unsubscribe(String topic, SseEmitter emitter) {
        // Returning `null` from `computeIfPresent` removes the mapping, under
        // the same lock `subscribe` takes -- so "was it empty" and "remove it"
        // cannot be separated by another thread's arrival. Leaving empty sets
        // behind instead is a leak whenever topics are per-request ids rather
        // than a fixed set of channels.
        subscribers.computeIfPresent(
                topic,
                (key, forTopic) -> {
                    forTopic.remove(emitter);
                    return forTopic.isEmpty() ? null : forTopic;
                });
    }

    /** How many connections are open, for a health endpoint or a metric. */
    public int openConnections() {
        return subscribers.values().stream().mapToInt(Set::size).sum();
    }

    /**
     * A comment frame every 15 seconds, so a proxy does not reap an idle
     * connection it thinks is dead.
     *
     * <p>This is the one that needs the pool size. Read the class Javadoc
     * before removing the property.
     */
    @Scheduled(fixedRate = 15_000)
    void heartbeat() {
        for (Map.Entry<String, Set<SseEmitter>> entry : subscribers.entrySet()) {
            for (SseEmitter emitter : entry.getValue()) {
                try {
                    emitter.send(SseEmitter.event().comment("keep-alive"));
                } catch (IOException | IllegalStateException gone) {
                    emitter.completeWithError(gone);
                }
            }
        }
    }

}
