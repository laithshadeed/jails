package {{pkg}};

{{extra}}{{key_import}}import java.util.List;
import java.util.Optional;
import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.BDDMockito.given;
import static org.mockito.Mockito.mock;

class {{name}}ServiceTest {

    private final {{name}}Repository repository = mock({{name}}Repository.class);
    private final {{name}}Service service = new {{name}}Service(repository);

    @Test
    void findAllDelegatesToThePort() {
        given(repository.findAll()).willReturn(List.of());

        assertThat(service.findAll()).isEmpty();
    }

    @Test
    void aMissingIdIsEmptyRatherThanNull() {
        {{key}} missing = {{absent}};
        given(repository.findById(missing)).willReturn(Optional.empty());

        assertThat(service.findById(missing)).isEmpty();
    }

    @Test
    void deleteReportsWhetherAnythingWasRemoved() {
        {{key}} removed = {{present}};
        {{key}} neverExisted = {{absent}};
        given(repository.deleteById(removed)).willReturn(true);
        given(repository.deleteById(neverExisted)).willReturn(false);

        assertThat(service.deleteById(removed)).isTrue();
        assertThat(service.deleteById(neverExisted)).isFalse();
    }
}
