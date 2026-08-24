package com.example.intercom.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.adapters.InMemoryMemberRepository;
import com.example.intercom.domain.Member;
import com.example.intercom.domain.MemberRole;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class CreateMemberUseCaseTest {

    private final InMemoryMemberRepository repository = new InMemoryMemberRepository();
    private final CreateMemberUseCase useCase = new DefaultCreateMemberUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        CreateMemberCommand command = new CreateMemberCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                "sample",
                "sample",
                MemberRole.values()[0]);

        Member created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.workspaceId()).isEqualTo(command.workspaceId());
        assertThat(created.email()).isEqualTo(command.email());
        assertThat(created.displayName()).isEqualTo(command.displayName());
        assertThat(created.role()).isEqualTo(command.role());
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }
}
