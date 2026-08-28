let token;

export function authorizationHeaders() {
  token = token || __ENV.AUTH_TOKEN;
  return token ? { Authorization: `Bearer ${token}` } : {};
}
