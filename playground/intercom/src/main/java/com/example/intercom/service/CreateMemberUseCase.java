package com.example.intercom.service;

import com.example.intercom.domain.Member;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface CreateMemberUseCase {

    Member execute(CreateMemberCommand command);
}
