import { check, sleep } from 'k6';
import { request, routes } from './api.js';

export const options = {
  vus: Number(__ENV.VUS || 10),
  duration: __ENV.DURATION || '30s',
  thresholds: {
    http_req_failed: ['rate<0.01'],
    http_req_duration: ['p(95)<500', 'p(99)<1000'],
  },
};

export default function () {
  const route = routes[__ITER % routes.length];
  const response = request(route);
  check(response, { 'status is below 500': (r) => r.status < 500 });
  sleep(0.1);
}
