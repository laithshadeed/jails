package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.dao.DataAccessException;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.transaction.annotation.Transactional;

/** Executable proof that this ownership relationship is a database invariant. */
@SpringBootTest
@Transactional
class ItemOwnerAssociationIT {

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
                .param("constraint", "items_item_owner_fk")
                .query(String.class)
                .single();

        assertThat(mapping).isEqualTo("owner_id=id");

        Boolean deferredUntilCommit = db.sql("""
                        select relation.condeferrable and relation.condeferred
                        from pg_constraint relation
                        where relation.contype = 'f' and relation.conname = :constraint
                        """)
                .param("constraint", "items_item_owner_fk")
                .query(Boolean.class)
                .single();
        assertThat(deferredUntilCommit).isTrue();
    }

    @Test
    void existingCrossBoundaryDataCannotPassConstraintValidation() {
        db.sql("alter table items drop constraint items_item_owner_fk").update();
        db.sql("""
                        alter table items
                        add constraint items_item_owner_fk
                        foreign key (owner_id)
                        references owners (id)
                        on update no action on delete no action
                        deferrable initially deferred
                        not valid
                        """).update();

        // Build the impossible historical row with referential triggers off,
        // then ask PostgreSQL itself to validate this exact named invariant.
        db.sql("set local session_replication_role = replica").update();
        db.sql("insert into items (id, owner_id, name, created_at) values ('90000000-0000-0000-0000-000000000009'::uuid, '90000000-0000-0000-0000-000000000009'::uuid, 'association-probe', timestamptz '2026-01-01 00:00:00+00')").update();
        db.sql("set local session_replication_role = origin").update();

        assertThatThrownBy(() -> db.sql(
                        "alter table items validate constraint items_item_owner_fk")
                .update())
                .isInstanceOf(DataAccessException.class)
                .rootCause()
                .hasMessageContaining("items_item_owner_fk");
    }
}
