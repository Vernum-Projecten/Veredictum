- Without JavaScript, the console's connect step offers the stored base URL
  again after a probe, so probing again reaches the same target instead of
  posting a blank address. A refused probe stores nothing, and its page
  offers no earlier address.
