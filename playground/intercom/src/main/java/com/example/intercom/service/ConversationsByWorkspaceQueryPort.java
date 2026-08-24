package com.example.intercom.service;

import com.example.intercom.domain.Conversation;
import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface ConversationsByWorkspaceQueryPort {

    List<Conversation> execute(ConversationsByWorkspaceQuery query);
}
