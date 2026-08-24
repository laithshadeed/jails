package com.example.intercom.service;

import com.example.intercom.domain.Member;
import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface MembersByWorkspaceQueryPort {

    List<Member> execute(MembersByWorkspaceQuery query);
}
