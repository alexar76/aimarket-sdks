type MockResponse = {
  status: number;
  body: string;
  headers?: Record<string, string>;
};

export class MockFetch {
  private readonly responses = new Map<string, MockResponse[]>();
  requestCount = 0;
  readonly requestLog: string[] = [];
  private failPostUntil = 0;
  private failPostCount = 0;

  expectGet(url: string, status: number, body = '{}', headers?: Record<string, string>): void {
    this.enqueue(`GET ${url}`, { status, body, headers });
  }

  expectPost(url: string, status: number, body = '{}', headers?: Record<string, string>): void {
    this.enqueue(`POST ${url}`, { status, body, headers });
  }

  private enqueue(key: string, response: MockResponse): void {
    const queue = this.responses.get(key) ?? [];
    queue.push(response);
    this.responses.set(key, queue);
  }

  failNextPost(count: number): void {
    this.failPostCount = 0;
    this.failPostUntil = count;
  }

  fetch: typeof fetch = async (input, init) => {
    this.requestCount++;
    const url = typeof input === 'string' ? input : input.toString();
    const method = init?.method ?? 'GET';
    const key = `${method} ${url}`;
    this.requestLog.push(key);

    if (method === 'POST' && this.failPostCount < this.failPostUntil) {
      this.failPostCount++;
      throw new TypeError('Simulated network failure');
    }

    const queue = this.responses.get(key);
    if (!queue || queue.length === 0) {
      return new Response('{"error":"not_found"}', {
        status: 404,
        headers: { 'content-type': 'application/json' },
      });
    }

    const mock = queue.length === 1 ? queue[0] : queue.shift()!;
    return new Response(mock.body, {
      status: mock.status,
      headers: { 'content-type': 'application/json', ...mock.headers },
    });
  };
}
