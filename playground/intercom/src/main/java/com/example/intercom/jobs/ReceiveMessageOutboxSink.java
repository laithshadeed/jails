package com.example.intercom.jobs;

import com.example.intercom.messaging.MessageReceivedEvent;

/** One independently configurable destination for a staged event. */
public interface ReceiveMessageOutboxSink {
    String name();
    void deliver(MessageReceivedEvent event);
}
