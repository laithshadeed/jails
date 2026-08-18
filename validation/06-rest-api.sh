#!/usr/bin/env bash
#
# Workout 6 -- the same application services over HTTP.
#   brief: stacks/workouts/06-rest-api.md
#
# Handlers stay thin: they parse, delegate to the service the CLI already
# uses, and map the answer to a status code. A stable error envelope
# distinguishes malformed JSON (400) from domain-invalid (422).
#
# jails features exercised:  g handler (the new one), add http, list<T>,
#                            capitalized types
#
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

start ledgerapi

run jails add json
run jails add testkit

# An HTTP server on com.sun.net.httpserver -- the JDK's own, no framework.
# The gym bans Express/Nest/Spring, and this mirrors the TypeScript flavor's
# bare Bun.serve. One virtual thread per request, so blocking JDBC in a
# handler is fine.
run jails add http

run jails g enum  Currency GBP EUR USD
run jails g value SourceRef system:string! externalId:string!

# The error envelope, verbatim from the brief:
#   { "error": { "code": "...", "message": "...", "details": {} } }
# Generated as a type so every handler returns the same shape instead of each
# one hand-rolling its own JSON.
run jails g value ApiError code:string! message:string! 'details:map<string,string>'

run jails g enum  WorkItemState OPEN IN_REVIEW APPROVED REJECTED
run jails g value WorkItem id:string! entityId:string! state:WorkItemState reason:string!

# A page of results. List endpoints need pagination and deterministic
# ordering, and every one of them wants this same envelope.
run jails g value Page 'items:list<WorkItem>' total:int offset:int limit:int

# The headline feature.
#
# `g handler <Name>` should emit an HttpHandler for a resource, wired into the
# server `add http` created, plus an integration test that drives it over a
# real loopback socket with java.net.http.HttpClient:
#
#   api/WorkItemHandler.java     routes GET /work-items, GET /work-items/{id},
#                                POST /work-items/{id}/approve; reads the body
#                                through Json; returns ApiError for failures
#   test/.../WorkItemHandlerTest.java
#                                starts the server on port 0, asserts status
#                                codes and the error envelope
#
# Thin by construction: the handler should take its service as a constructor
# argument, so the same code path serves CLI and HTTP.
run jails g handler WorkItem
run jails g handler Import
run jails g handler Reconciliation

fixtures 06-rest-api import-payload.json malformed.json domain-invalid.json \
                     idempotency-conflict.json pagination-list.json

# ---- assertions ------------------------------------------------------------

section "add http" "JDK server, no framework"
exists "$API/Server.java" 'server generated'
has   "$API/Server.java" 'com.sun.net.httpserver' "the JDK's own HTTP server"
has   "$API/Server.java" 'VirtualThread|newVirtualThreadPerTaskExecutor' 'one virtual thread per request'
for forbidden in org.springframework jakarta.servlet io.netty; do
  lacks "$API/Server.java" "$forbidden" "no $forbidden"
done

section "g handler" "thin handlers in api/"
exists "$API/WorkItemHandler.java" 'handler generated'
has "$API/WorkItemHandler.java" 'HttpHandler|HttpExchange' 'implements the JDK handler interface'
has "$API/WorkItemHandler.java" '/work-items'              'routes its resource path'
# Thin means delegating: a handler holding business logic is the thing the
# brief is warning against.
has "$API/WorkItemHandler.java" 'private final|this\.'     'takes its service as a dependency'
lacks "$API/WorkItemHandler.java" 'java.sql'               'no SQL in a handler'

section "g handler" "status codes and envelope"
has "$API/WorkItemHandler.java" '400'      'malformed JSON -> 400'
has "$API/WorkItemHandler.java" '404'      'missing resource -> 404'
has "$API/WorkItemHandler.java" '422'      'domain-invalid -> 422'
has "$API/WorkItemHandler.java" 'ApiError' 'failures use the shared envelope'

section "g handler" "integration test over a real socket"
exists "$TEST/api/WorkItemHandlerTest.java" 'handler test generated'
has "$TEST/api/WorkItemHandlerTest.java" 'java.net.http|HttpClient' 'drives it over HTTP'
# Port 0 lets the OS pick a free port -- a hardcoded one makes the suite
# flaky the moment anything else is listening.
has "$TEST/api/WorkItemHandlerTest.java" '\(0\)|port 0|ephemeral'   'binds an ephemeral port'

section "map<K,V>" "error details"
has "$DOMAIN/ApiError.java" 'Map<String, ?String>' 'map<string,string> -> Map<String, String>'
has "$DOMAIN/ApiError.java" 'Map.copyOf|copyOf'    'map component is defensively copied'

section "pagination" "list envelope"
has "$DOMAIN/Page.java" 'List<WorkItem> items' 'page carries typed items'
has "$DOMAIN/Page.java" 'int total'            'total is a primitive int'

section "more than one handler" "generator is not single-use"
exists "$API/ImportHandler.java"         'second handler'
exists "$API/ReconciliationHandler.java" 'third handler'

build

verdict "workout 6 (rest api)"
