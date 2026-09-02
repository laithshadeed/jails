package {{pkg}};

import java.time.Duration;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.jdbc.core.simple.JdbcClient;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * The one property an in-memory presence map cannot have: two adapters are two
 * nodes, one records a join and the <em>other</em> is asked. A module-level
 * dict answers empty here and says nothing about why.
 */
{{annotation}}{{container_annotation}}@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class Jdbc{{name}}PresenceIT {

    @Autowired private JdbcClient db;

    @Test
    void aMemberJoinedOnOneNodeIsPresentOnTheOther() {
        var first = new Jdbc{{name}}Presence(db, Duration.ofSeconds(30));
        var second = new Jdbc{{name}}Presence(db, Duration.ofSeconds(30));

        first.join("room", "member-1");

        assertThat(second.present("room")).containsExactly("member-1");
    }

    /** Keying on the member alone would let either disconnect erase the other. */
    @Test
    void oneNodeLeavingDoesNotEvictTheSameMemberOnAnother() {
        var first = new Jdbc{{name}}Presence(db, Duration.ofSeconds(30));
        var second = new Jdbc{{name}}Presence(db, Duration.ofSeconds(30));
        first.join("room", "member-1");
        second.join("room", "member-1");

        first.leave("room", "member-1");

        assertThat(second.present("room")).containsExactly("member-1");
        second.leave("room", "member-1");
        assertThat(first.present("room")).isEmpty();
    }

    /** A node that dies never sends {@code leave}; the window is what makes that self-correcting. */
    @Test
    void aClaimNobodyRefreshedStopsCountingWithoutAnyoneLeaving() {
        var node = new Jdbc{{name}}Presence(db, Duration.ofSeconds(30));
        node.join("room", "member-1");
        db.sql("update {{table}} set seen_at = now() - make_interval(secs => 120)").update();

        assertThat(node.present("room")).isEmpty();
    }
}
