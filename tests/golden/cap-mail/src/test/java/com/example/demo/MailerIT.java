package com.example.demo;

import static org.assertj.core.api.Assertions.assertThat;

import jakarta.mail.Folder;
import jakarta.mail.Message;
import jakarta.mail.Session;
import jakarta.mail.Store;
import java.time.Duration;
import java.util.Arrays;
import java.util.Properties;
import org.awaitility.Awaitility;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.test.context.DynamicPropertyRegistry;
import org.springframework.test.context.DynamicPropertySource;
import org.testcontainers.containers.GenericContainer;
import org.testcontainers.junit.jupiter.Container;
import org.testcontainers.junit.jupiter.Testcontainers;

/**
 * Send a message and read it back, which is the only assertion worth making.
 *
 * <p>A mail test that checks {@code send()} did not throw proves almost
 * nothing: a wrong From, a wrong recipient, an empty subject and a message the
 * server silently drops all pass it. This is the shape Spring Boot's own
 * {@code MailSenderAutoConfigurationIntegrationTests} uses — Mailpit over SMTP,
 * then POP3 to read the delivered messages back.
 *
 * <p><b>Properties are registered, not injected by a service connection.</b>
 * There is no {@code @ServiceConnection} for mail in Boot 4 — grep the source,
 * there is no {@code MailConnectionDetails} — so the host and port have to be
 * bound with {@code @DynamicPropertySource}. That is the difference from
 * {@code add db}, and the reason this test does not follow that shape.
 *
 * <p>The read-back is polled rather than asserted immediately: SMTP acceptance
 * and POP3 visibility are two separate events on the server, and asserting
 * between them is the flake that makes people delete mail tests.
 */
@SpringBootTest
@Testcontainers(disabledWithoutDocker = true)
class MailerIT {

    private static final int SMTP_PORT = 1025;
    private static final int POP3_PORT = 1110;

    @Container
    private static final GenericContainer<?> mailpit =
            new GenericContainer<>("axllent/mailpit:v1.21")
                    .withExposedPorts(SMTP_PORT, POP3_PORT)
                    .withEnv("MP_POP3_AUTH", "user:pass");

    @DynamicPropertySource
    static void mailProperties(DynamicPropertyRegistry registry) {
        registry.add("spring.mail.host", mailpit::getHost);
        registry.add("spring.mail.port", () -> mailpit.getMappedPort(SMTP_PORT));
    }

    @Autowired private Mailer mailer;

    @Test
    void a_sent_message_arrives_with_the_subject_it_was_given() throws Exception {
        String subject = "hello from the integration test";

        mailer.send("to@example.com", subject, "the body");

        assertThat(subjectsWaiting()).contains(subject);
    }

    private java.util.List<String> subjectsWaiting() throws Exception {
        Properties properties = new Properties();
        Session session = Session.getInstance(properties);
        try (Store store = session.getStore("pop3")) {
            store.connect(mailpit.getHost(), mailpit.getMappedPort(POP3_PORT), "user", "pass");
            try (Folder folder = store.getFolder("inbox")) {
                folder.open(Folder.READ_ONLY);
                Awaitility.await()
                        .atMost(Duration.ofSeconds(10))
                        .ignoreExceptions()
                        .until(() -> folder.getMessageCount() > 0);
                return Arrays.stream(folder.getMessages()).map(MailerIT::subjectOf).toList();
            }
        }
    }

    private static String subjectOf(Message message) {
        try {
            return message.getSubject();
        } catch (jakarta.mail.MessagingException unreadable) {
            throw new IllegalStateException(unreadable);
        }
    }
}
