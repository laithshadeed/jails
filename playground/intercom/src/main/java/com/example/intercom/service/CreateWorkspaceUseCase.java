package com.example.intercom.service;

import com.example.intercom.domain.Workspace;

/** A single application operation, independent of HTTP and persistence adapters. */
@FunctionalInterface
public interface CreateWorkspaceUseCase {

    Workspace execute(CreateWorkspaceCommand command);
}
