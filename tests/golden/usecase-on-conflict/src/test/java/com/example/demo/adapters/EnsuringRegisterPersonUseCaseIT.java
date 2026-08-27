package com.example.demo.adapters;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.demo.TestcontainersConfig;
import com.example.demo.domain.Person;
import com.example.demo.service.RegisterPersonCommand;
import com.example.demo.service.RegisterPersonUseCase;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.context.annotation.Import;

@Import(TestcontainersConfig.class)
@SpringBootTest
@org.springframework.transaction.annotation.Transactional
class EnsuringRegisterPersonUseCaseIT {

    @Autowired
    private RegisterPersonUseCase useCase;

    /**
     * The behaviour the whole kind exists for, and the one an in-memory fake
     * could not prove: {@code on conflict} is the database's decision, so only
     * the database can be asked whether it was made.
     */
    @Test
    void twoCallsWithTheSameKeyAreOneRow() {
        RegisterPersonCommand command = new RegisterPersonCommand(
                "sample");

        Person first = useCase.execute(command);
        Person second = useCase.execute(command);

        assertThat(second).isEqualTo(first);
        assertThat(second.email()).isEqualTo(first.email());
    }
}
