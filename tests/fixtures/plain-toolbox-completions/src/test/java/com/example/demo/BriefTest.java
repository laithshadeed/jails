package com.example.demo;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalArgumentException;

import com.example.demo.adapters.CsvReader;
import com.example.demo.domain.CanonicalTransaction;
import com.example.demo.domain.Currency;
import com.example.demo.domain.SourceRef;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.LocalDate;
import java.util.Optional;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

@DisplayName("brief")
class BriefTest {

    @TempDir
    Path temp;

    @Test
    @DisplayName("parses a quoted value")
    void parsesAQuotedValue() throws Exception {
        var csv = temp.resolve("quoted.csv");
        Files.writeString(csv, "memo,amount\n\"coffee, lunch\",1200\n");

        var rows = CsvReader.read(csv);

        assertThat(rows).hasSize(1);
        assertThat(rows.getFirst().get("memo")).isEqualTo("coffee, lunch");
    }

    @Test
    @DisplayName("rejects blank ids")
    void rejectsBlankIds() {
        assertThatIllegalArgumentException()
                .isThrownBy(() -> CanonicalTransaction.of(
                        "  ",
                        LocalDate.of(2026, 8, 1),
                        1_200L,
                        Currency.GBP,
                        new SourceRef("bank", "123"),
                        Optional.empty()))
                .withMessageContaining("id");
    }
}
