export async function exec(argv) {
  let response;
  try {
    response = await fetch('/api/exec', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
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
