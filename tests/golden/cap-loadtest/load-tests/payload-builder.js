// Add representative route-specific bodies here. The fallback is valid JSON,
// so adding a generated route never breaks the load-test runner itself.
export function payloadFor(route) {
  return { route: route.handler, value: 'sample' };
}
