package com.example.webcrawler.service;

import static org.assertj.core.api.Assertions.assertThat;

import com.example.webcrawler.adapters.InMemoryCrawlRunRepository;
import com.example.webcrawler.domain.CrawlRun;
import java.net.URI;
import java.util.UUID;
import org.junit.jupiter.api.Test;

class QueueCrawlUseCaseTest {

    private final InMemoryCrawlRunRepository repository = new InMemoryCrawlRunRepository();
    private final QueueCrawlUseCase useCase = new DefaultQueueCrawlUseCase(repository);

    @Test
    void createsAndPersistsTheResource() {
        QueueCrawlCommand command = new QueueCrawlCommand(
                UUID.fromString("00000000-0000-0000-0000-000000000001"),
                URI.create("https://example.com"));

        CrawlRun created = useCase.execute(command);

        assertThat(created.id()).isNotNull();
        assertThat(created.id()).isEqualTo(command.id());
        assertThat(created.seedUrl()).isEqualTo(command.seedUrl());
        assertThat(repository.findById(String.valueOf(created.id()))).contains(created);
    }
}
