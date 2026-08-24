package com.example.intercom.service;

import com.example.intercom.domain.InboxMember;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface AddInboxMemberUseCase {

    InboxMember execute(AddInboxMemberCommand command);
}
