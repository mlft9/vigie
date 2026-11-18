// File d'envoi de la saisie vers le cœur.
//
// Deux appels IPC asynchrones peuvent être exécutés dans le désordre côté Rust : envoyer
// chaque frappe indépendamment risquerait d'inverser des caractères. La file n'a donc qu'un
// envoi en cours à la fois ; ce qui arrive entre-temps est regroupé et part au suivant.
// Les gros collages sont découpés pour respecter la limite du cœur (64 Kio par appel).

/** Caractères par envoi : 16 384 caractères font au plus 64 Kio une fois encodés en UTF-8. */
export const MAX_CHARS_PER_SEND = 16 * 1024;

type Send = (data: string, binary: boolean) => Promise<void>;

interface Chunk {
  data: string;
  binary: boolean;
}

export class InputQueue {
  private readonly pending: Chunk[] = [];
  private sending = false;
  private closed = false;

  constructor(
    private readonly send: Send,
    private readonly onError: (error: unknown) => void,
  ) {}

  /** Ajoute une saisie. `binary` : octets bruts (rapports souris de certains modes). */
  push(data: string, binary = false): void {
    if (this.closed || data === '') return;
    const last = this.pending.at(-1);
    if (last?.binary === binary) {
      last.data += data;
    } else {
      this.pending.push({ data, binary });
    }
    void this.drain();
  }

  /** Abandonne la saisie en attente ; les ajouts suivants sont ignorés. */
  close(): void {
    this.closed = true;
    this.pending.length = 0;
  }

  private async drain(): Promise<void> {
    if (this.sending) return;
    this.sending = true;
    try {
      for (let chunk = this.pending.at(0); chunk && !this.closed; chunk = this.pending.at(0)) {
        const part = sliceChars(chunk.data, MAX_CHARS_PER_SEND);
        chunk.data = chunk.data.slice(part.length);
        if (chunk.data === '') this.pending.shift();
        await this.send(part, chunk.binary);
      }
    } catch (error) {
      this.pending.length = 0;
      this.onError(error);
    } finally {
      this.sending = false;
    }
  }
}

/** Découpe sans couper une paire de substitution UTF-16 (emoji, caractères rares). */
function sliceChars(text: string, max: number): string {
  if (text.length <= max) return text;
  const code = text.charCodeAt(max - 1);
  const end = code >= 0xd800 && code <= 0xdbff ? max - 1 : max;
  return text.slice(0, end);
}
