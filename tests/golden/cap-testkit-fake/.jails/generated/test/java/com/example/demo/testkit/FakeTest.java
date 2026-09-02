package com.example.demo.testkit;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.util.List;
import org.junit.jupiter.api.Test;

class FakeTest {

    @Test
    void playsEachStepInOrder() {
        var fake = Fake.of(Fake.value("first"), Fake.value("second"));

        assertThat(fake.next()).isEqualTo("first");
        assertThat(fake.next()).isEqualTo("second");
    }

    @Test
    void repeatsTheLastStepOnceTheScriptRunsOut() {
        var fake = Fake.of(Fake.value("only"));

        assertThat(fake.next()).isEqualTo("only");
        assertThat(fake.next()).isEqualTo("only");
    }

    @Test
    void throwsWhateverTheScriptSaysToThrow() {
        var fake = Fake.<String>of(Fake.failure(new IllegalStateException("simulated timeout")));

        assertThatThrownBy(fake::next).isInstanceOf(IllegalStateException.class).hasMessage("simulated timeout");
    }

    @Test
    void recordsHowItWasCalled() {
        var fake = Fake.of(Fake.value(1));

        fake.next("a", 2);
        fake.next("b");

        assertThat(fake.calls()).containsExactly(List.of("a", 2), List.of("b"));
        assertThat(fake.callCount()).isEqualTo(2);
    }

    @Test
    void rejectsAnEmptyScript() {
        assertThatThrownBy(Fake::of).isInstanceOf(IllegalArgumentException.class);
    }
}
