export async function publish(event: Event) {
  await broker.send(event);
  return { ok: true };
}