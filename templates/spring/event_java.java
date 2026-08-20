package {{pkg}};

import java.time.Instant;

/**
 * What crosses the topic.
 *
 * <p>A record of its own, not a domain type. A message is a published
 * contract that outlives the process that sent it -- consumers read messages
 * written by older versions -- so it needs to change on its own schedule.
 * Reusing the domain type couples every consumer to an internal refactor.
 *
 * <p>{@code occurredAt} is on the event rather than inferred from the
 * broker: the time something happened and the time it was published are
 * different facts, and only the first one survives a replay.
 */
public record {{name}}Event(String id, Instant occurredAt) {}
