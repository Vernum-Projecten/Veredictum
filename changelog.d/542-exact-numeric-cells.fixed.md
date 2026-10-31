- A numeric AQL result cell equals another only when both denote one value
  exactly: integers compare as integers, so two that differ beyond the 53-bit
  float mantissa no longer compare equal, and an integer equals a float only
  when the float is integral and denotes it.
