package {{pkg}};

import {{clients}}.{{fetcher}}Fetcher;
import {{clients}}.{{fetcher}}Fetcher.FetchException;
import {{clients}}.{{fetcher}}Fetcher.FetchedResource;
import io.micrometer.core.instrument.Counter;
import io.micrometer.core.instrument.MeterRegistry;
import java.io.IOException;
import java.io.StringReader;
import java.net.IDN;
import java.net.URI;
import java.net.URISyntaxException;
import java.nio.charset.StandardCharsets;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;
import java.util.UUID;
import javax.swing.text.MutableAttributeSet;
import javax.swing.text.html.HTML;
import javax.swing.text.html.HTMLEditorKit;
import javax.swing.text.html.parser.ParserDelegator;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.jdbc.core.simple.JdbcClient;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;
import org.springframework.transaction.support.TransactionTemplate;

/**
 * Durable, exact-origin, robots-aware HTTP graph traversal.
 *
 * <p>The entire frontier is PostgreSQL state. Claims have expiring leases,
 * URLs are canonical primary keys, retries are bounded, and run cancellation
 * is persisted, so process restarts do not lose or duplicate progress.
 */
@Component
public final class {{name}}Workflow {

    static final String ROBOTS_PATH = "/robots.txt";

    private final JdbcClient db;
    private final TransactionTemplate transactions;
    private final {{fetcher}}Fetcher fetcher;
    private final int maxConfiguredPages;
    private final int maxConfiguredDepth;
    private final int maxAttempts;
    private final int leaseSeconds;
    private final MeterRegistry meters;

    public {{name}}Workflow(
            JdbcClient db,
            TransactionTemplate transactions,
            {{fetcher}}Fetcher fetcher,
            @Value("${jails.http-workflows.{{property}}.max-pages:1000}") int maxConfiguredPages,
            @Value("${jails.http-workflows.{{property}}.max-depth:10}") int maxConfiguredDepth,
            @Value("${jails.http-workflows.{{property}}.max-attempts:5}") int maxAttempts,
            @Value("${jails.http-workflows.{{property}}.lease-seconds:30}") int leaseSeconds,
            MeterRegistry meters) {
        this.db = Objects.requireNonNull(db, "db is required");
        this.transactions = Objects.requireNonNull(transactions, "transactions are required");
        this.fetcher = Objects.requireNonNull(fetcher, "fetcher is required");
        if (maxConfiguredPages < 1 || maxConfiguredDepth < 0 || maxAttempts < 1 || leaseSeconds < 1) {
            throw new IllegalArgumentException("workflow limits are invalid");
        }
        this.maxConfiguredPages = maxConfiguredPages;
        this.maxConfiguredDepth = maxConfiguredDepth;
        this.maxAttempts = maxAttempts;
        this.leaseSeconds = leaseSeconds;
        this.meters = Objects.requireNonNull(meters, "meter registry is required");
    }

    public RunStatus start(StartRequest request) {
        Objects.requireNonNull(request, "request is required");
        URI seed = canonical(request.seedUrl());
        if (request.maxPages() < 1 || request.maxPages() > maxConfiguredPages) {
            throw new IllegalArgumentException("maxPages must be between 1 and " + maxConfiguredPages);
        }
        if (request.maxDepth() < 0 || request.maxDepth() > maxConfiguredDepth) {
            throw new IllegalArgumentException("maxDepth must be between 0 and " + maxConfiguredDepth);
        }
        return transactions.execute(status -> {
            int inserted = db.sql("""
                            insert into {{table}}_runs
                                (id, seed_url, origin_scheme, origin_host, origin_port, status,
                                 max_pages, max_depth, pages_visited, cancel_requested, created_at)
                            values (:id, :seed, :scheme, :host, :port, 'QUEUED',
                                    :maxPages, :maxDepth, 0, false, now())
                            on conflict (id) do nothing
                            """)
                    .param("id", request.id()).param("seed", seed.toString())
                    .param("scheme", seed.getScheme()).param("host", seed.getHost())
                    .param("port", effectivePort(seed)).param("maxPages", request.maxPages())
                    .param("maxDepth", request.maxDepth()).update();
            if (inserted == 0) {
                var existing = status(request.id()).orElseThrow();
                if (!existing.seedUrl().equals(seed)
                        || existing.maxPages() != request.maxPages()
                        || existing.maxDepth() != request.maxDepth()) {
                    throw new IdempotencyConflictException(request.id());
                }
                return existing;
            }
            URI robots = origin(seed).resolve(ROBOTS_PATH);
            db.sql("""
                            insert into {{table}}_frontier
                                (run_id, url, depth, kind, state, attempts, max_attempts, next_attempt_at)
                            values (:id, :url, -1, 'POLICY', 'PENDING', 0, :maxAttempts, now())
                            """)
                    .param("id", request.id()).param("url", robots.toString())
                    .param("maxAttempts", maxAttempts).update();
            counter("started").increment();
            return status(request.id()).orElseThrow();
        });
    }

    public Optional<RunStatus> status(UUID id) {
        return db.sql("""
                        select id, seed_url, status, max_pages, max_depth, pages_visited,
                               cancel_requested, last_error, created_at, started_at, finished_at
                        from {{table}}_runs where id = :id
                        """)
                .param("id", id)
                .query((rows, rowNumber) -> new RunStatus(
                        rows.getObject("id", UUID.class), URI.create(rows.getString("seed_url")),
                        RunState.valueOf(rows.getString("status")), rows.getInt("max_pages"),
                        rows.getInt("max_depth"), rows.getInt("pages_visited"),
                        rows.getBoolean("cancel_requested"),
                        Optional.ofNullable(rows.getString("last_error")),
                        instant(rows, "created_at"), optionalInstant(rows, "started_at"),
                        optionalInstant(rows, "finished_at")))
                .optional();
    }

    public List<Page> pages(UUID id) {
        return db.sql("""
                        select url, depth, status_code, content_type, discovered_at
                        from {{table}}_pages where run_id = :id order by discovered_at, url
                        """)
                .param("id", id)
                .query((rows, rowNumber) -> new Page(
                        URI.create(rows.getString("url")), rows.getInt("depth"),
                        rows.getInt("status_code"), rows.getString("content_type"),
                        instant(rows, "discovered_at")))
                .list();
    }

    public RunStatus cancel(UUID id) {
        int changed = db.sql("""
                        update {{table}}_runs
                        set cancel_requested = true, status = 'CANCELLED', finished_at = now()
                        where id = :id and status in ('QUEUED','RUNNING')
                        """).param("id", id).update();
        if (changed > 0) {
            db.sql("""
                            update {{table}}_frontier set state = 'CANCELLED', lease_until = null
                            where run_id = :id and state in ('PENDING','RUNNING')
                            """).param("id", id).update();
            counter("cancelled").increment();
        }
        return status(id).orElseThrow(() -> new NotFoundException(id));
    }

    @Scheduled(
            fixedDelayString = "${jails.http-workflows.{{property}}.delay:PT0.25S}",
            initialDelayString = "${jails.http-workflows.{{property}}.initial-delay:PT1S}")
    public void runOnce() {
        Claim claim = transactions.execute(status -> claim());
        if (claim == null) return;
        try {
            FetchedResource resource = claim.kind() == Kind.POLICY
                    ? fetcher.fetch(claim.url(), Set.of(404, 410))
                    : fetcher.fetch(claim.url());
            Completion completion = claim.kind() == Kind.POLICY
                    ? new Completion(resource, Set.of(), robotsText(resource))
                    : new Completion(resource, extractLinks(resource), null);
            transactions.executeWithoutResult(status -> complete(claim, completion));
        } catch (FetchException failure) {
            transactions.executeWithoutResult(status -> fail(claim, failure, failure.retryable()));
        } catch (RuntimeException failure) {
            transactions.executeWithoutResult(status -> fail(claim, failure, true));
        }
    }

    private Claim claim() {
        var claimed = db.sql("""
                        with candidate as (
                            select frontier.run_id, frontier.url
                            from {{table}}_frontier frontier
                            join {{table}}_runs runs on runs.id = frontier.run_id
                            where runs.status in ('QUEUED','RUNNING') and not runs.cancel_requested
                              and ((frontier.state = 'PENDING' and frontier.next_attempt_at <= now())
                                or (frontier.state = 'RUNNING' and frontier.lease_until <= now()))
                            order by runs.created_at, frontier.depth, frontier.url
                            for update of frontier skip locked limit 1
                        )
                        update {{table}}_frontier frontier
                        set state = 'RUNNING', attempts = frontier.attempts + 1,
                            lease_until = now() + make_interval(secs => :leaseSeconds)
                        from candidate
                        where frontier.run_id = candidate.run_id and frontier.url = candidate.url
                        returning frontier.run_id, frontier.url, frontier.depth,
                                  frontier.kind, frontier.attempts, frontier.max_attempts
                        """)
                .param("leaseSeconds", leaseSeconds)
                .query((rows, rowNumber) -> new Claim(
                        rows.getObject("run_id", UUID.class), URI.create(rows.getString("url")),
                        rows.getInt("depth"), Kind.valueOf(rows.getString("kind")),
                        rows.getInt("attempts"), rows.getInt("max_attempts")))
                .optional().orElse(null);
        if (claimed != null) {
            db.sql("""
                            update {{table}}_runs set status = 'RUNNING',
                                started_at = coalesce(started_at, now())
                            where id = :id and status = 'QUEUED'
                            """).param("id", claimed.runId()).update();
        }
        return claimed;
    }

    private void complete(Claim claim, Completion completion) {
        RunData run = lockRun(claim.runId());
        if (run.cancelRequested()) {
            markFrontier(claim, "CANCELLED", null);
            return;
        }
        if (claim.kind() == Kind.POLICY) {
            db.sql("update {{table}}_runs set robots_rules = :rules where id = :id")
                    .param("rules", completion.robots()).param("id", claim.runId()).update();
            URI seed = canonical(run.seed());
            if (robotsAllowed(completion.robots(), seed)) {
                enqueue(claim.runId(), seed, 0, Kind.PAGE);
            }
        } else {
            int inserted = db.sql("""
                            insert into {{table}}_pages
                                (run_id, url, depth, status_code, content_type, discovered_at)
                            values (:runId, :url, :depth, :status, :contentType, now())
                            on conflict (run_id, url) do nothing
                            """)
                    .param("runId", claim.runId()).param("url", claim.url().toString())
                    .param("depth", claim.depth()).param("status", completion.resource().statusCode())
                    .param("contentType", completion.resource().contentType()).update();
            if (inserted > 0) {
                db.sql("update {{table}}_runs set pages_visited = pages_visited + 1 where id = :id")
                        .param("id", claim.runId()).update();
            }
            int visited = run.pagesVisited() + inserted;
            if (visited < run.maxPages() && claim.depth() < run.maxDepth()) {
                int scheduled = scheduledPages(claim.runId());
                for (URI candidate : completion.links()) {
                    if (scheduled >= run.maxPages()) break;
                    Optional<URI> accepted = withinOrigin(candidate, run);
                    if (accepted.isPresent() && robotsAllowed(run.robotsRules(), accepted.orElseThrow())) {
                        scheduled += enqueue(claim.runId(), accepted.orElseThrow(), claim.depth() + 1, Kind.PAGE);
                    }
                }
            }
        }
        markFrontier(claim, "SUCCEEDED", null);
        finishIfDone(claim.runId());
        counter(claim.kind() == Kind.POLICY ? "policy" : "page").increment();
    }

    private void fail(Claim claim, RuntimeException failure, boolean retryable) {
        String error = String.valueOf(failure.getMessage());
        if (error.length() > 4000) error = error.substring(0, 4000);
        if (retryable && claim.attempt() < claim.maxAttempts()) {
            db.sql("""
                            update {{table}}_frontier
                            set state = 'PENDING', lease_until = null, last_error = :error,
                                next_attempt_at = now() + make_interval(
                                    secs => least(300, cast(power(2, attempts) as integer)))
                            where run_id = :runId and url = :url and state = 'RUNNING'
                            """).param("error", error).param("runId", claim.runId())
                    .param("url", claim.url().toString()).update();
            counter("retry").increment();
            return;
        }
        markFrontier(claim, "FAILED", error);
        db.sql("""
                        update {{table}}_runs set status = 'FAILED', last_error = :error, finished_at = now()
                        where id = :id and status in ('QUEUED','RUNNING')
                        """).param("error", error).param("id", claim.runId()).update();
        db.sql("""
                        update {{table}}_frontier set state = 'CANCELLED', lease_until = null
                        where run_id = :id and state in ('PENDING','RUNNING')
                        """).param("id", claim.runId()).update();
        counter("failed").increment();
    }

    private RunData lockRun(UUID id) {
        return db.sql("""
                        select seed_url, origin_scheme, origin_host, origin_port, max_pages,
                               max_depth, pages_visited, robots_rules, cancel_requested
                        from {{table}}_runs where id = :id for update
                        """).param("id", id)
                .query((rows, rowNumber) -> new RunData(
                        URI.create(rows.getString("seed_url")), rows.getString("origin_scheme"),
                        rows.getString("origin_host"), rows.getInt("origin_port"),
                        rows.getInt("max_pages"), rows.getInt("max_depth"),
                        rows.getInt("pages_visited"), rows.getString("robots_rules"),
                        rows.getBoolean("cancel_requested")))
                .single();
    }

    private int enqueue(UUID runId, URI uri, int depth, Kind kind) {
        return db.sql("""
                        insert into {{table}}_frontier
                            (run_id, url, depth, kind, state, attempts, max_attempts, next_attempt_at)
                        values (:runId, :url, :depth, :kind, 'PENDING', 0, :maxAttempts, now())
                        on conflict (run_id, url) do nothing
                        """).param("runId", runId).param("url", uri.toString())
                .param("depth", depth).param("kind", kind.name())
                .param("maxAttempts", maxAttempts).update();
    }

    private int scheduledPages(UUID runId) {
        return db.sql("select count(*) from {{table}}_frontier where run_id = :id and kind = 'PAGE'")
                .param("id", runId).query(Integer.class).single();
    }

    private void markFrontier(Claim claim, String state, String error) {
        db.sql("""
                        update {{table}}_frontier set state = :state, lease_until = null, last_error = :error
                        where run_id = :runId and url = :url and state = 'RUNNING'
                        """).param("state", state).param("error", error)
                .param("runId", claim.runId()).param("url", claim.url().toString()).update();
    }

    private void finishIfDone(UUID runId) {
        boolean remaining = db.sql("""
                        select exists(select 1 from {{table}}_frontier
                                      where run_id = :id and state in ('PENDING','RUNNING'))
                        """).param("id", runId).query(Boolean.class).single();
        if (!remaining) {
            db.sql("""
                            update {{table}}_runs set status = 'SUCCEEDED', finished_at = now()
                            where id = :id and status in ('QUEUED','RUNNING')
                            """).param("id", runId).update();
        }
    }

    private static Set<URI> extractLinks(FetchedResource resource) {
        var links = new LinkedHashSet<URI>();
        var source = new String(resource.body(), StandardCharsets.UTF_8);
        try {
            new ParserDelegator().parse(new StringReader(source), new HTMLEditorKit.ParserCallback() {
                @Override public void handleStartTag(HTML.Tag tag, MutableAttributeSet attributes, int position) {
                    if (tag == HTML.Tag.A) add(attributes);
                }
                @Override public void handleSimpleTag(HTML.Tag tag, MutableAttributeSet attributes, int position) {
                    if (tag == HTML.Tag.A) add(attributes);
                }
                private void add(MutableAttributeSet attributes) {
                    Object href = attributes.getAttribute(HTML.Attribute.HREF);
                    if (href == null) return;
                    try { links.add(canonical(resource.uri().resolve(String.valueOf(href)))); }
                    catch (RuntimeException ignored) { /* a malformed link is not a failed page */ }
                }
            }, true);
        } catch (IOException impossibleForStringReader) {
            throw new IllegalStateException("could not parse in-memory HTML", impossibleForStringReader);
        }
        return Set.copyOf(links);
    }

    private static String robotsText(FetchedResource resource) {
        return resource.statusCode() == 404 || resource.statusCode() == 410
                ? "" : new String(resource.body(), StandardCharsets.UTF_8);
    }

    private static boolean robotsAllowed(String source, URI uri) {
        if (source == null || source.isBlank()) return true;
        var rules = new ArrayList<Rule>();
        boolean applies = false;
        boolean sawDirective = false;
        for (String raw : source.split("\\R")) {
            String line = raw.split("#", 2)[0].trim();
            int colon = line.indexOf(':');
            if (colon < 0) continue;
            String key = line.substring(0, colon).trim().toLowerCase(Locale.ROOT);
            String value = line.substring(colon + 1).trim();
            if (key.equals("user-agent")) {
                if (sawDirective) { applies = false; sawDirective = false; }
                if (value.equals("*")) applies = true;
            } else if (key.equals("allow") || key.equals("disallow")) {
                sawDirective = true;
                if (applies && !value.isEmpty()) rules.add(new Rule(value, key.equals("allow")));
            }
        }
        String target = uri.getRawPath() + (uri.getRawQuery() == null ? "" : "?" + uri.getRawQuery());
        Rule winner = null;
        for (Rule rule : rules) {
            if (target.startsWith(rule.path())
                    && (winner == null || rule.path().length() > winner.path().length()
                    || (rule.path().length() == winner.path().length() && rule.allow()))) {
                winner = rule;
            }
        }
        return winner == null || winner.allow();
    }

    private static Optional<URI> withinOrigin(URI candidate, RunData run) {
        try {
            URI normalized = canonical(candidate);
            return normalized.getScheme().equals(run.scheme())
                    && normalized.getHost().equalsIgnoreCase(run.host())
                    && effectivePort(normalized) == run.port()
                    ? Optional.of(normalized) : Optional.empty();
        } catch (RuntimeException rejected) {
            return Optional.empty();
        }
    }

    private static URI canonical(URI candidate) {
        Objects.requireNonNull(candidate, "uri is required");
        String scheme = candidate.getScheme() == null ? "" : candidate.getScheme().toLowerCase(Locale.ROOT);
        if (!(scheme.equals("http") || scheme.equals("https"))
                || candidate.getHost() == null || candidate.getUserInfo() != null) {
            throw new IllegalArgumentException("only absolute http(s) URLs without user-info are allowed");
        }
        try {
            return new URI(scheme, null, IDN.toASCII(candidate.getHost()).toLowerCase(Locale.ROOT),
                    candidate.getPort(), candidate.getRawPath().isEmpty() ? "/" : candidate.getRawPath(),
                    candidate.getRawQuery(), null).normalize();
        } catch (URISyntaxException failure) {
            throw new IllegalArgumentException("URL cannot be canonicalized", failure);
        }
    }

    private static URI origin(URI uri) {
        try { return new URI(uri.getScheme(), null, uri.getHost(), uri.getPort(), "/", null, null); }
        catch (URISyntaxException impossible) { throw new IllegalArgumentException(impossible); }
    }

    private static int effectivePort(URI uri) {
        return uri.getPort() >= 0 ? uri.getPort() : (uri.getScheme().equals("https") ? 443 : 80);
    }

    private Counter counter(String outcome) {
        return Counter.builder("http.workflow.items")
                .tag("workflow", "{{property}}").tag("outcome", outcome).register(meters);
    }

    private static Instant instant(java.sql.ResultSet rows, String column) throws java.sql.SQLException {
        return rows.getObject(column, OffsetDateTime.class).toInstant();
    }

    private static Optional<Instant> optionalInstant(java.sql.ResultSet rows, String column)
            throws java.sql.SQLException {
        return Optional.ofNullable(rows.getObject(column, OffsetDateTime.class)).map(OffsetDateTime::toInstant);
    }

    public enum RunState { QUEUED, RUNNING, SUCCEEDED, FAILED, CANCELLED }
    private enum Kind { POLICY, PAGE }
    public record StartRequest(UUID id, URI seedUrl, int maxPages, int maxDepth) {
        public StartRequest {
            Objects.requireNonNull(id, "id is required");
            Objects.requireNonNull(seedUrl, "seedUrl is required");
        }
    }
    public record RunStatus(UUID id, URI seedUrl, RunState state, int maxPages, int maxDepth,
                            int pagesVisited, boolean cancelRequested, Optional<String> lastError,
                            Instant createdAt, Optional<Instant> startedAt, Optional<Instant> finishedAt) {}
    public record Page(URI url, int depth, int statusCode, String contentType, Instant discoveredAt) {}
    private record Claim(UUID runId, URI url, int depth, Kind kind, int attempt, int maxAttempts) {}
    private record Completion(FetchedResource resource, Set<URI> links, String robots) {}
    private record RunData(URI seed, String scheme, String host, int port, int maxPages, int maxDepth,
                           int pagesVisited, String robotsRules, boolean cancelRequested) {}
    private record Rule(String path, boolean allow) {}

    public static final class NotFoundException extends RuntimeException {
        public NotFoundException(UUID id) { super("workflow run not found: " + id); }
    }
    public static final class IdempotencyConflictException extends RuntimeException {
        public IdempotencyConflictException(UUID id) {
            super("workflow id " + id + " was already used with different input");
        }
    }
}
