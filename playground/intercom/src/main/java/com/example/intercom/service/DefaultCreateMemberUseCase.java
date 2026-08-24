package com.example.intercom.service;

import com.example.intercom.app.MemberRepository;
import com.example.intercom.domain.Member;
import java.time.Instant;
import java.util.Objects;
import org.springframework.stereotype.Component;
import org.springframework.transaction.annotation.Transactional;

/** The conventional implementation generated from the target record's field model. */
@Component
public class DefaultCreateMemberUseCase implements CreateMemberUseCase {

    private final MemberRepository repository;

    public DefaultCreateMemberUseCase(MemberRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    @Transactional
    @Override
    public Member execute(CreateMemberCommand command) {
        Objects.requireNonNull(command, "command is required");
        Member member = new Member(
                command.id(),
                command.workspaceId(),
                command.email(),
                command.displayName(),
                command.role(),
                Instant.now(),
                Instant.now());
        repository.save(member);
        return member;
    }
}
