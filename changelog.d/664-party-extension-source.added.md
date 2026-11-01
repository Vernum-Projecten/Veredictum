- A statement may declare extension routes under `extensions`, each keyed by
  an extension the catalogue defines (`ehr-import` today) and naming the
  party's own OpenAPI document and the operation in it. The catalogue names
  such a route with an `x-veredictum-extension` source description that only
  a shared setup may call, and a case whose setups need an extension the
  statement does not declare is skipped as `extension-not-offered`.
  `validate` refuses an unknown extension key, an extension call outside
  `cases/_setup/`, and any citation, judge or rows file on an extension step,
  under the new `arazzo-extension` gate.
