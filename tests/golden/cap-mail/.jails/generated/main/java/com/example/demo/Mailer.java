package com.example.demo;

import org.springframework.mail.SimpleMailMessage;
import org.springframework.mail.MailException;
import org.springframework.mail.javamail.JavaMailSender;
import org.springframework.stereotype.Component;

/**
 * Sending mail, with the two things that are wrong by default made explicit.
 *
 * <p><b>{@code spring.mail.host} has no default that fails loudly.</b> Boot
 * leaves it unset, JavaMail falls back to {@code localhost:25}, and a
 * deployment that forgot to configure the host does not fail at startup — it
 * fails at the first send, per message, in whatever thread happened to be
 * sending. The generated properties set it explicitly for local development and
 * name it in the comment, so "where does mail go" has an answer in the file
 * rather than in JavaMail's defaults.
 *
 * <p><b>The From address is not the sender's to choose per call.</b> A
 * hard-coded {@code from} at each call site drifts, and the one that drifts is
 * the one a receiving mail server rejects for failing SPF. It is configuration
 * here, set once.
 *
 * <p>Sending is synchronous on purpose. An {@code @Async} wrapper looks like an
 * improvement and hides the failure: the caller returns success, the send
 * throws on another thread, and nobody finds out. If mail must not block the
 * request, put it behind {@code jails g durable-job}, which retries and reports.
 */
@Component
public class Mailer {

    private final JavaMailSender sender;
    private final String from;

    public Mailer(
            JavaMailSender sender,
            @org.springframework.beans.factory.annotation.Value("${app.mail.from}") String from) {
        this.sender = sender;
        this.from = from;
    }

    /**
     * @throws MailException when the message could not be handed to the mail
     *     server. Deliberately not swallowed: a send that failed and reported
     *     success is the failure this class exists to make visible.
     */
    public void send(String to, String subject, String body) {
        SimpleMailMessage message = new SimpleMailMessage();
        message.setFrom(from);
        message.setTo(to);
        message.setSubject(subject);
        message.setText(body);
        sender.send(message);
    }
}
