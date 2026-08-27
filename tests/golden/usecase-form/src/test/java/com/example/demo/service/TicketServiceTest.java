package com.example.demo.service;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.BDDMockito.given;
import static org.mockito.Mockito.mock;

import com.example.demo.app.TicketRepository;
import java.util.List;
import java.util.Optional;
import org.junit.jupiter.api.Test;

class TicketServiceTest {

    private final TicketRepository repository = mock(TicketRepository.class);
    private final TicketService service = new TicketService(repository);

    @Test
    void findAllDelegatesToThePort() {
        given(repository.findAll()).willReturn(List.of());

        assertThat(service.findAll()).isEmpty();
    }

    @Test
    void aMissingIdIsEmptyRatherThanNull() {
        Long missing = 2L;
        given(repository.findById(missing)).willReturn(Optional.empty());

        assertThat(service.findById(missing)).isEmpty();
    }

    @Test
    void deleteReportsWhetherAnythingWasRemoved() {
        Long removed = 1L;
        Long neverExisted = 2L;
        given(repository.deleteById(removed)).willReturn(true);
        given(repository.deleteById(neverExisted)).willReturn(false);

        assertThat(service.deleteById(removed)).isTrue();
        assertThat(service.deleteById(neverExisted)).isFalse();
    }
}
