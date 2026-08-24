package com.example.intercom.service;

import com.example.intercom.app.InboxMemberRepository;
import com.example.intercom.domain.InboxMember;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultAddInboxMemberUseCase implements AddInboxMemberUseCase {

    private final InboxMemberRepository repository;

    public DefaultAddInboxMemberUseCase(InboxMemberRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public InboxMember execute(AddInboxMemberCommand command) {
        Objects.requireNonNull(command, "command is required");
        InboxMember inboxMember = new InboxMember(
                command.id(),
                command.workspaceId(),
                command.inboxId(),
                command.memberId(),
                Instant.now(),
                Instant.now());
        repository.save(inboxMember);
        return inboxMember;
    }
}
