package com.example.webcrawler.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.webcrawler.adapters.InMemoryCrawledPageRepository;
import com.example.webcrawler.domain.CrawledPage;
import java.net.URI;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class RecordCrawledPageUseCaseTest {

    private final InMemoryCrawledPageRepository repository = new InMemoryCrawledPageRepository();
    private final RecordCrawledPageUseCase useCase = new DefaultRecordCrawledPageUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        RecordCrawledPageCommand command = new RecordCrawledPageCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                URI.create("https://example.com"),
                1);

        CrawledPage created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.crawlRunId()).isEqualTo(command.crawlRunId());
        assertThat(created.url()).isEqualTo(command.url());
        assertThat(created.statusCode()).isEqualTo(command.statusCode());
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }
}
