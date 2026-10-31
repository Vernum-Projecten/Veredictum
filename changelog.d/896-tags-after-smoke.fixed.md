- The release pipeline applies the image tags, `:latest` among them, and
  deploys the hosted console only after the image smoke passed, so a pushed
  digest that does not serve the console stays untagged.
