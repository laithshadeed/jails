package com.example.demo.service;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.BDDMockito.given;
import static org.mockito.Mockito.mock;

import com.example.demo.app.WidgetRepository;
import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class WidgetServiceTest {

    private final WidgetRepository repository = mock(WidgetRepository.class);
    private final WidgetService service = new WidgetService(repository);

    @Test
    void findAllDelegatesToThePort() {
        given(repository.findAll()).willReturn(List.of());

        assertThat(service.findAll()).isEmpty();
    }

    @Test
    void aMissingIdIsEmptyRatherThanNull() {
        UUID missing = UUID.fromString("00000000-0000-0000-0000-000000000002");
        given(repository.findById(missing)).willReturn(Optional.empty());

        assertThat(service.findById(missing)).isEmpty();
    }

    @Test
    void deleteReportsWhetherAnythingWasRemoved() {
        UUID removed = UUID.fromString("00000000-0000-0000-0000-000000000001");
        UUID neverExisted = UUID.fromString("00000000-0000-0000-0000-000000000002");
        given(repository.deleteById(removed)).willReturn(true);
        given(repository.deleteById(neverExisted)).willReturn(false);

        assertThat(service.deleteById(removed)).isTrue();
        assertThat(service.deleteById(neverExisted)).isFalse();
    }
}
