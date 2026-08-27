package com.example.demo.domain;

/**
 * The SenderType values this application understands.
 *
 * <p>A closed set, so a switch over it is checked for exhaustiveness and
 * adding a constant makes the compiler point at every place that has to
 * handle it.
 */
public enum SenderType {
    CUSTOMER,
    ADMIN
}
