package com.example.intercom.adapters;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import com.example.intercom.TestcontainersConfig;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;
import org.springframework.dao.DataAccessException;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.transaction.annotation.Transactional;

/** Executable proof that this ownership relationship is a database invariant. */
@Import(TestcontainersConfig.class)
@SpringBootTest
@Transactional
class InboxWorkspaceAssociationIT {

    @Autowired private JdbcClient db;

    @Test
    void schemaCarriesTheExactOrderedCompositeRelationship() {
        String mapping = db.sql("""
                        select string_agg(child_column.attname || '=' || parent_column.attname,
                                         ',' order by pair.ordinality)
                        from pg_constraint relation
                        cross join lateral unnest(relation.conkey, relation.confkey)
                            with ordinality as pair(child_number, parent_number, ordinality)
                        join pg_attribute child_column
                          on child_column.attrelid = relation.conrelid
                         and child_column.attnum = pair.child_number
                        join pg_attribute parent_column
                          on parent_column.attrelid = relation.confrelid
                         and parent_column.attnum = pair.parent_number
                        where relation.contype = 'f' and relation.conname = :constraint
                        """)
                .param("constraint", "inboxes_inbox_workspace_fk")
                .query(String.class)
                .single();

        assertThat(mapping).isEqualTo("workspace_id=id");

        Boolean deferredUntilCommit = db.sql("""
                        select relation.condeferrable and relation.condeferred
                        from pg_constraint relation
                        where relation.contype = 'f' and relation.conname = :constraint
                        """)
                .param("constraint", "inboxes_inbox_workspace_fk")
                .query(Boolean.class)
                .single();
        assertThat(deferredUntilCommit).isTrue();
    }

    @Test
    void existingCrossBoundaryDataCannotPassConstraintValidation() {
        db.sql("alter table inboxes drop constraint inboxes_inbox_workspace_fk").update();
        db.sql("""
                        alter table inboxes
                        add constraint inboxes_inbox_workspace_fk
                        foreign key (workspace_id)
                        references workspaces (id)
                        on update no action on delete no action
                        deferrable initially deferred
                        not valid
                        """).update();

        // Build the impossible historical row with referential triggers off,
        // then ask PostgreSQL itself to validate this exact named invariant.
        db.sql("set local session_replication_role = replica").update();
        db.sql("insert into inboxes (id, workspace_id, name, channel, created_at, updated_at) values ('90000000-0000-0000-0000-000000000009'::uuid, '90000000-0000-0000-0000-000000000009'::uuid, 'association-probe', 'association-probe', timestamptz '2026-01-01 00:00:00+00', timestamptz '2026-01-01 00:00:00+00')").update();
        db.sql("set local session_replication_role = origin").update();

        assertThatThrownBy(() -> db.sql(
                        "alter table inboxes validate constraint inboxes_inbox_workspace_fk")
                .update())
                .isInstanceOf(DataAccessException.class)
                .rootCause()
                .hasMessageContaining("inboxes_inbox_workspace_fk");
    }
}
