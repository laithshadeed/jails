package com.example.demo.service;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.BDDMockito.given;
import static org.mockito.Mockito.mock;

import com.example.demo.app.MessageRepository;
import java.util.List;
import java.util.Optional;
import org.junit.jupiter.api.Test;

class MessageServiceTest {

    private final MessageRepository repository = mock(MessageRepository.class);
    private final MessageService service = new MessageService(repository);

    @Test
    void findAllDelegatesToThePort() {
        given(repository.findAll()).willReturn(List.of());

        assertThat(service.findAll()).isEmpty();
    }

    @Test
    void aMissingIdIsEmptyRatherThanNull() {
        given(repository.findById("nope")).willReturn(Optional.empty());

        assertThat(service.findById("nope")).isEmpty();
    }

    @Test
    void deleteReportsWhetherAnythingWasRemoved() {
        given(repository.deleteById("gone")).willReturn(true);
        given(repository.deleteById("never-existed")).willReturn(false);

        assertThat(service.deleteById("gone")).isTrue();
        assertThat(service.deleteById("never-existed")).isFalse();
    }
}
