package com.example.intercom.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.intercom.adapters.InMemoryInboxMemberRepository;
import com.example.intercom.domain.InboxMember;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class AddInboxMemberUseCaseTest {

    private final InMemoryInboxMemberRepository repository = new InMemoryInboxMemberRepository();
    private final AddInboxMemberUseCase useCase = new DefaultAddInboxMemberUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        AddInboxMemberCommand command = new AddInboxMemberCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"));

        InboxMember created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.workspaceId()).isEqualTo(command.workspaceId());
        assertThat(created.inboxId()).isEqualTo(command.inboxId());
        assertThat(created.memberId()).isEqualTo(command.memberId());
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }
}
