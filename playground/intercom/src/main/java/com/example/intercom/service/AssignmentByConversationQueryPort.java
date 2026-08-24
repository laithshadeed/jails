package com.example.intercom.service;

import com.example.intercom.domain.ConversationAssignment;
import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface AssignmentByConversationQueryPort {

    List<ConversationAssignment> execute(AssignmentByConversationQuery query);
}
