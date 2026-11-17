// Fake Claude Code CLI used by tests. Emulates `claude -p --output-format
// stream-json` output for success, errors, usage limits and slow runs.
const args = process.argv.slice(2);
const val = (flag) => {
  const i = args.indexOf(flag);
  return i >= 0 ? args[i + 1] : undefined;
};
const out = (o) => process.stdout.write(JSON.stringify(o) + '\n');

if (args[0] === 'auth' && args[1] === 'status') {
  out({ loggedIn: true, authMethod: 'claude.ai', apiProvider: 'firstParty', subscriptionType: 'max', email: 'hidden@example.com' });
  process.exit(0);
}
if (args[0] === '--version') {
  console.log('9.9.9 (Claude Code)');
  process.exit(0);
}

let prompt = '';
process.stdin.on('data', (d) => (prompt += d));
process.stdin.on('end', async () => {
  const resume = val('--resume');
  const session = resume || val('--session-id') || 'generated';
  const env = Object.keys(process.env).filter((k) => /^(ANTHROPIC_|CLAUDE_|CLAUDECODE)/i.test(k) && k !== 'CLAUDE_CONFIG_DIR');
  if (resume === 'gone') {
    out({ type: 'result', subtype: 'error_during_execution', is_error: true, result: 'No conversation found with session ID: gone', session_id: 'gone' });
    process.exit(1);
  }
  out({ type: 'system', subtype: 'init', session_id: session, model: 'claude-fake-1', tools: [] });
  if (prompt.includes('TRIGGER_LIMIT')) {
    out({ type: 'rate_limit_event', rate_limit_info: { status: 'rejected', resetsAt: 1794938400, rateLimitType: 'five_hour' } });
    out({ type: 'result', subtype: 'success', is_error: true, result: 'Claude AI usage limit reached|1794938400', session_id: session });
    process.exit(1);
  }
  if (prompt.includes('TRIGGER_AUTH')) {
    out({ type: 'result', subtype: 'success', is_error: true, result: 'Invalid API key · Please run /login', session_id: session });
    process.exit(1);
  }
  if (prompt.includes('TRIGGER_CRASH')) {
    process.stderr.write('boom: something broke\n');
    process.exit(2);
  }
  if (prompt.includes('TRIGGER_SLOW')) {
    await new Promise((r) => setTimeout(r, 60000));
  }
  const reply = `session=${session} resumed=${!!resume} leaked_env=${env.length} tools=${val('--tools') === '' ? 'none' : 'some'} prompt=${prompt}`;
  for (let i = 0; i < reply.length; i += 16) {
    out({ type: 'stream_event', event: { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: reply.slice(i, i + 16) } }, parent_tool_use_id: null });
  }
  out({ type: 'assistant', message: { content: [{ type: 'text', text: reply }] }, parent_tool_use_id: null });
  out({ type: 'result', subtype: 'success', is_error: false, result: reply, session_id: session, terminal_reason: 'completed' });
});
