package com.example.demo;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.cli.LedgerCli;
import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import org.junit.jupiter.api.Test;

class CheckoutIT {

    @Test
    void worksEndToEnd() {
        var out = new ByteArrayOutputStream();
        var err = new ByteArrayOutputStream();

        var status = LedgerCli.run(LedgerCli.commands(), new PrintStream(out), new PrintStream(err), "greet", "world");

        assertThat(status).isZero();
        assertThat(out.toString()).isEqualTo("world" + System.lineSeparator());
        assertThat(err.toString()).isEmpty();
    }
}
