package com.example.intercom.service;

import com.example.intercom.domain.InboxMember;
import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface InboxMembersByInboxQueryPort {

    List<InboxMember> execute(InboxMembersByInboxQuery query);
}
