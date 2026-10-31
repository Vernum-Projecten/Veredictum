- A console call whose body runs past the transport cap is answered `413`
  with a sentence naming the cap, instead of `400` "One of its arguments could
  not be decoded". The console counts the body bytes it read to tell the two
  apart, rather than reading the framework's error message.
