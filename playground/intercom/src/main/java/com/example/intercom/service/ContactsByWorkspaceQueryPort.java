package com.example.intercom.service;

import com.example.intercom.domain.Contact;
import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface ContactsByWorkspaceQueryPort {

    List<Contact> execute(ContactsByWorkspaceQuery query);
}
