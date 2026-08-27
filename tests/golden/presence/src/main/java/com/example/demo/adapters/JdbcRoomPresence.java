package com.example.demo.adapters;

import com.example.demo.app.RoomPresence;
import java.time.Duration;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

/**
 * Presence in the database, so two nodes give one answer.
 *
 * <p>A row per {@code (scope, member, node)}, because a member connected twice
 * is present until both claims are gone. A {@code seen_at} window rather than
 * a leave-only protocol, because a process that dies never sends
 * {@code leave}. {@code jails explain presence} has the rest.
 */
@Component
public class JdbcRoomPresence implements RoomPresence {

    /** This process. Not the hostname: a restart would inherit the dead one's claims. */
    private final String node = UUID.randomUUID().toString();

    private final JdbcClient db;
    private final Duration window;

    public JdbcRoomPresence(
            JdbcClient db,
            @Value("${presence.room.window:PT30S}") Duration window) {
        this.db = Objects.requireNonNull(db, "db is required");
        if (window.isZero() || window.isNegative()) {
            throw new IllegalArgumentException("presence window must be positive");
        }
        this.window = window;
    }

    @Override
    public void join(String scope, String member) {
        heartbeat(scope, member);
    }

    @Override
    public void heartbeat(String scope, String member) {
        db.sql("""
                        insert into room_presence (scope, member, node, seen_at)
                        values (:scope, :member, :node, now())
                        on conflict (scope, member, node) do update set seen_at = now()
                        """)
                .param("scope", scope)
                .param("member", member)
                .param("node", node)
                .update();
    }

    @Override
    public void leave(String scope, String member) {
        db.sql("delete from room_presence where scope = :scope and member = :member and node = :node")
                .param("scope", scope)
                .param("member", member)
                .param("node", node)
                .update();
    }

    @Override
    public List<String> present(String scope) {
        return db.sql("""
                        select distinct member
                        from room_presence
                        where scope = :scope and seen_at > now() - make_interval(secs => :seconds)
                        order by member
                        """)
                .param("scope", scope)
                .param("seconds", window.toSeconds())
                .query(String.class)
                .list();
    }

    /** Storage only -- {@link #present} already filters by age. Without it the table grows forever. */
    @Scheduled(fixedDelayString = "${presence.room.sweep:PT60S}")
    public void sweep() {
        db.sql("delete from room_presence where seen_at < now() - make_interval(secs => :seconds)")
                .param("seconds", window.toSeconds() * 2)
                .update();
    }
}
