/** Base exception for AI Market Protocol errors. */
export class AimarketException extends Error {
  readonly statusCode?: number;

  constructor(message: string, statusCode?: number) {
    super(message);
    this.name = 'AimarketException';
    this.statusCode = statusCode;
  }
}

/** Network-level error: timeout, connection refused, DNS failure, etc. */
export class AimarketNetworkException extends AimarketException {
  constructor(message: string, statusCode?: number) {
    super(message, statusCode);
    this.name = 'AimarketNetworkException';
  }
}

/** Payment failure: depleted channel, insufficient funds, or expired credit. */
export class AimarketPaymentException extends AimarketException {
  constructor(message: string, statusCode = 402) {
    super(message, statusCode);
    this.name = 'AimarketPaymentException';
  }
}

/** Safety gate blocked the invocation. */
export class AimarketSafetyException extends AimarketException {
  readonly reason: string;

  constructor(reason: string) {
    super(`Safety blocked: ${reason}`, 403);
    this.name = 'AimarketSafetyException';
    this.reason = reason;
  }
}
