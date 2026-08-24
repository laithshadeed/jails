package com.example.intercom.service;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.BDDMockito.given;
import static org.mockito.Mockito.mock;

import com.example.intercom.app.ConversationRepository;
import java.util.List;
import java.util.Optional;
import org.junit.jupiter.api.Test;

class ConversationServiceTest {

    private final ConversationRepository repository = mock(ConversationRepository.class);
    private final ConversationService service = new ConversationService(repository);

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
