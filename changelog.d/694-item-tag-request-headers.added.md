- The ITEM_TAG catalogue sends the `openehr-item-tag` and
  `openehr-version-item-tag` request headers on a COMPOSITION create, a
  COMPOSITION update and an EHR_STATUS update, and reads each tag back from
  the target the header names: the VERSIONED_OBJECT for `openehr-item-tag`,
  the new VERSION for `openehr-version-item-tag`. Two further workflows pin a
  header carrying two tags and an empty header that removes the container's
  tags.
