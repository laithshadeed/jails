package com.example.demo.service;

import com.example.demo.domain.Item;
import java.util.List;

/** Read-side port; the application contract contains no JDBC or HTTP types. */
@FunctionalInterface
public interface ItemsByOwnerEmailQuery {

    List<Item> execute(ItemsByOwnerEmailCriteria criteria);
}
