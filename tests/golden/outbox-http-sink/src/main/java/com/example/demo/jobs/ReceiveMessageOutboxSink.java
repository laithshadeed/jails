package com.example.demo.jobs;

import com.example.demo.messaging.MessageReceivedEvent;

/** One independently configurable destination for a staged event. */
public interface ReceiveMessageOutboxSink {
    String name();
    void deliver(MessageReceivedEvent event);
}
