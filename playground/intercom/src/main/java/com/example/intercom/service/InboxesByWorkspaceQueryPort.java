package com.example.intercom.service;

import com.example.intercom.domain.Inbox;
import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface InboxesByWorkspaceQueryPort {

    List<Inbox> execute(InboxesByWorkspaceQuery query);
}
