- The console closes every connection in stages (RFC 9112 §9.6): it
  half-closes, then reads and discards what the caller still sends for up to
  30 seconds. A body refused past the transport cap now reaches the caller as
  the refusal instead of a connection reset.
