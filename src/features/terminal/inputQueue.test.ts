import { InputQueue, MAX_CHARS_PER_SEND } from './inputQueue';

function deferred() {
  let resolve: () => void = () => undefined;
  let reject: (e: unknown) => void = () => undefined;
  const promise = new Promise<void>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("file d'envoi de la saisie", () => {
  it("n'a qu'un envoi en cours et regroupe la saisie suivante, dans l'ordre", async () => {
    const calls: [string, boolean][] = [];
    const pending = [deferred(), deferred()];
    const queue = new InputQueue((data, binary) => {
      calls.push([data, binary]);
      return pending[calls.length - 1]?.promise ?? Promise.resolve();
    }, vi.fn());

    queue.push('a');
    queue.push('b');
    queue.push('c');
    expect(calls).toEqual([['a', false]]);

    pending[0]?.resolve();
    await flush();
    expect(calls).toEqual([
      ['a', false],
      ['bc', false],
    ]);
  });

  it('sépare texte et octets bruts', async () => {
    const calls: [string, boolean][] = [];
    const gate = deferred();
    const queue = new InputQueue((data, binary) => {
      calls.push([data, binary]);
      return calls.length === 1 ? gate.promise : Promise.resolve();
    }, vi.fn());

    queue.push('a');
    queue.push('\x1b[M', true);
    queue.push('b');
    gate.resolve();
    await flush();
    expect(calls).toEqual([
      ['a', false],
      ['\x1b[M', true],
      ['b', false],
    ]);
  });

  it('découpe les gros collages sans couper un emoji', async () => {
    const calls: string[] = [];
    const queue = new InputQueue((data) => {
      calls.push(data);
      return Promise.resolve();
    }, vi.fn());
    const text = 'a'.repeat(MAX_CHARS_PER_SEND - 1) + '😀' + 'b'.repeat(10);
    queue.push(text);
    await flush();
    await flush();
    expect(calls.join('')).toBe(text);
    expect(calls[0]).toHaveLength(MAX_CHARS_PER_SEND - 1);
    for (const call of calls) expect(call.length).toBeLessThanOrEqual(MAX_CHARS_PER_SEND);
  });

  it("vide la file et signale l'erreur si un envoi échoue", async () => {
    const onError = vi.fn();
    const send = vi.fn().mockRejectedValueOnce({ code: 'ssh.session_closed' });
    const queue = new InputQueue(send, onError);
    queue.push('a');
    await flush();
    expect(onError).toHaveBeenCalledWith({ code: 'ssh.session_closed' });
  });

  it('ignore la saisie après fermeture', async () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const queue = new InputQueue(send, vi.fn());
    queue.close();
    queue.push('a');
    await flush();
    expect(send).not.toHaveBeenCalled();
  });
});
