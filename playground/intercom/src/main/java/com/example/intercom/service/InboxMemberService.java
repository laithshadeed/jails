package com.example.intercom.service;

import com.example.intercom.app.InboxMemberRepository;
import com.example.intercom.domain.InboxMember;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link InboxMember}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class InboxMemberService {

    private final InboxMemberRepository repository;

    public InboxMemberService(InboxMemberRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<InboxMember> findAll() {
        return repository.findAll();
    }

    public Optional<InboxMember> findById(String id) {
        return repository.findById(id);
    }

    public InboxMember create(InboxMember inboxMember) {
        repository.save(inboxMember);
        return inboxMember;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
