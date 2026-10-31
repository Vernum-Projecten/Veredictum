- The console no longer sends its own failure details to the browser: an
  internal failure names only its kind, and the detail (which can name paths
  on the host) goes to the instance's log. A base URL carrying a user name or
  password is refused under every posture, without repeating it, so a
  credential typed into the address never reaches the draft or a refusal.
