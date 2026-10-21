export async function exec(argv) {
  const headers = { 'content-type': 'application/json' };
  const token = sessionStorage.getItem('bucky.token');
  if (token) headers.authorization = `Bearer ${token}`;
  let response;
  try {
    response = await fetch('/api/exec', {
      method: 'POST',
      headers,
      body: JSON.stringify({ argv }),
    });
  } catch {
    throw new Error('unavailable');
  }
  const text = await response.text();
  let body;
  try {
    body = JSON.parse(text);
  } catch {
    throw new Error('unavailable');
  }
  if (!response.ok && !body.summary) throw new Error('unavailable');
  return {
    ok: Boolean(body.ok),
    summary: body.summary || '',
    data: body.data || {},
  };
}
