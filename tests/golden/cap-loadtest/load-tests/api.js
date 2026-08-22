import http from 'k6/http';
import { payloadFor } from './payload-builder.js';
import { authorizationHeaders } from './token-cache.js';

const baseUrl = __ENV.BASE_URL || 'http://localhost:8080';

export const routes = [
  { method: "GET", path: "/health", handler: "HealthController#get" }
];

export function request(route) {
  const params = { headers: { ...authorizationHeaders() } };
  if (['POST', 'PUT', 'PATCH'].includes(route.method)) {
    params.headers['Content-Type'] = 'application/json';
    return http.request(route.method, `${baseUrl}${route.path}`, JSON.stringify(payloadFor(route)), params);
  }
  return http.request(route.method, `${baseUrl}${route.path}`, null, params);
}
