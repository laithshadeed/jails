package com.example.intercom.service;

import com.example.intercom.domain.Message;
import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface MessagesByConversationQueryPort {

    List<Message> execute(MessagesByConversationQuery query);
}
