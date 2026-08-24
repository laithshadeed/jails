package com.example.intercom.service;

import com.example.intercom.app.MemberRepository;
import com.example.intercom.domain.Member;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import org.springframework.stereotype.Component;

/**
 * What the application can do with {@link Member}.
 *
 * <p>Depends on the port, not on an adapter, so a test can hand it an
 * in-memory implementation and never start a database.
 */
@Component
public class MemberService {

    private final MemberRepository repository;

    public MemberService(MemberRepository repository) {
        this.repository = Objects.requireNonNull(repository, "repository is required");
    }

    public List<Member> findAll() {
        return repository.findAll();
    }

    public Optional<Member> findById(String id) {
        return repository.findById(id);
    }

    public Member create(Member member) {
        repository.save(member);
        return member;
    }

    /** @return true when a row was actually removed. */
    public boolean deleteById(String id) {
        return repository.deleteById(id);
    }
}
