package com.example.demo.service;

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;

class BillingServiceTest {

    @Test
    void instantiates() {
        assertThat(new BillingService()).isNotNull();
    }
}
