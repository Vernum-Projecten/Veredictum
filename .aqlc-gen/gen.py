import sys, os, glob, json, re, yaml
sys.path.insert(0, os.path.dirname(__file__))
from table import T, REASONS, ISSUES
from cites import CITES, RM_ROOT, OK200, BAD400

EXT = sys.argv[1]; REPO = sys.argv[2]; FOLLOWUP = sys.argv[3]
COMMIT = 'd5b8c554fecef73fbeab0620dfd28d4a1149bd63'

class Lit(str): pass
def lit_repr(d, s): return d.represent_scalar('tag:yaml.org,2002:str', s, style='|')
class D(yaml.SafeDumper):
    def ignore_aliases(self, data): return not isinstance(data, (dict, list))
    def increase_indent(self, flow=False, indentless=False): return super().increase_indent(flow, False)
D.add_representer(Lit, lit_repr)
def dump(o): return yaml.dump(o, Dumper=D, sort_keys=False, width=100, allow_unicode=True, default_flow_style=False)

cases = {}
for p in sorted(glob.glob(EXT + '/corpus/cases/**/*.yaml', recursive=True)):
    d = yaml.safe_load(open(p)); cases[d['id']] = (os.path.relpath(p, EXT), d)
assert len(cases) == 236, len(cases)
assert set(cases) == set(T), (set(cases) ^ set(T))

def cite(key):
    s, sec, q = CITES[key] if isinstance(key, str) else key
    return {'source': s, 'section': sec, 'quote': q}

FIX = EXT + '/corpus/fixtures/'
TEMPLATES = {'vital_signs.opt': 'aqlc_vital_signs', 'vital_signs_sectioned.opt': 'aqlc_vital_signs_sectioned',
             'type_repetition_conformance_ehrbase.org.opt': 'aqlc_type_repetition'}
compositions = {}
def comp(name):
    if name not in compositions: compositions[name] = json.load(open(FIX + 'compositions/' + name))
    return compositions[name]
statuses = {}
def status(name):
    if name not in statuses: statuses[name] = json.load(open(FIX + 'ehr_status/' + name))
    return statuses[name]

COMMITTED = {'source': 'specs/openehr/ITS-REST/specifications/responses/201_COMPOSITION.yaml', 'section': 'description',
             'quote': '`201 Created` is returned when the COMPOSITION is successfully created.'}
EHR_CREATED = {'source': 'specs/openehr/ITS-REST/specifications/responses/201_EHR.yaml', 'section': 'description',
               'quote': '`201 Created` is returned when the EHR has been successfully created,'}
STATUS_READ = {'source': 'specs/openehr/ITS-REST/specifications/responses/200_EHR_STATUS_retrieved.yaml', 'section': 'description',
               'quote': '`200 OK` is returned when the requested EHR_STATUS is successfully retrieved.'}

seeds = {}  # signature -> (name, workflow)
def seed_for(case):
    arr = case['arrange']; uses_status = 'status_uid' in json.dumps(case)
    sig = json.dumps(arr, sort_keys=True) + str(uses_status)
    if sig in seeds: return seeds[sig][0]
    parts = []; deps = []; steps = []; outputs = {}; inputs = {}
    for s in arr:
        k, v = next(iter(s.items()))
        if k == 'upload_template':
            deps.append(f'upload_template-{TEMPLATES[v]}')
        elif k == 'create_ehr':
            a = v['alias']; inp = 'ehr_id' if a == 'ehr1' else a + '_id'
            inputs[inp] = {'type': 'string', 'format': 'uuid'}
            st = {'stepId': f'create-{a}', 'description': f'Create the EHR {a} with the generated {inp}.',
                  'operationId': '$sourceDescriptions.ehr.ehr_create_with_id',
                  'parameters': [{'name': 'ehr_id', 'in': 'path', 'value': f'$inputs.{inp}'}]}
            if 'status' in v:
                st['description'] = f'Create the EHR {a} with the generated {inp} and the EHR_STATUS {v["status"]}.'
                st['parameters'].append({'name': 'Content-Type', 'in': 'header', 'value': 'application/json'})
                st['requestBody'] = {'contentType': 'application/json', 'payload': status(v['status'])}
                parts.append(v['status'].replace('.json', ''))
            st['successCriteria'] = [{'condition': '$statusCode == 201', 'x-veredictum-cite': EHR_CREATED}]
            steps.append(st); outputs[inp] = f'$inputs.{inp}'
            if a != 'ehr1': parts.append('ehr2')
        elif k == 'commit_composition':
            a = v['alias']; e = v['ehr']; inp = 'ehr_id' if e == 'ehr1' else e + '_id'
            steps.append({'stepId': f'commit-{a}', 'description': f'Commit {v["file"]} as {a} in {e}, asking for the full representation to learn its version uid.',
                          'operationId': '$sourceDescriptions.ehr.composition_create',
                          'parameters': [{'name': 'ehr_id', 'in': 'path', 'value': f'$inputs.{inp}'},
                                         {'name': 'Prefer', 'in': 'header', 'value': 'return=representation'}],
                          'requestBody': {'contentType': 'application/json', 'payload': comp(v['file'])},
                          'successCriteria': [{'condition': '$statusCode == 201', 'x-veredictum-cite': COMMITTED}],
                          'outputs': {'uid': '$response.body#/uid/value'}})
            outputs[a] = f'$steps.commit-{a}.outputs.uid'
            parts.append(v['file'].replace('.json', ''))
        else:
            raise SystemExit(f'arrange {k} in a carried case')
    if uses_status:
        steps.append({'stepId': 'status', 'description': 'Read the EHR_STATUS of ehr1 to learn its version uid.',
                      'operationId': '$sourceDescriptions.ehr.ehr_status_get_at_time',
                      'parameters': [{'name': 'ehr_id', 'in': 'path', 'value': '$inputs.ehr_id'},
                                     {'name': 'Accept', 'in': 'header', 'value': 'application/json'}],
                      'successCriteria': [{'condition': '$statusCode == 200', 'x-veredictum-cite': STATUS_READ}],
                      'outputs': {'uid': '$response.body#/uid/value'}})
        outputs['status_uid'] = '$steps.status.outputs.uid'
    name = 'seed-' + ('-'.join(parts) if parts else 'empty_ehr') + ('-status' if uses_status else '')
    assert not any(name == s[0] for s in seeds.values()), name
    wf = {'workflowId': name,
          'summary': 'An EHR holding ' + (', '.join(parts) if parts else 'nothing but its EHR_STATUS'),
          'description': 'Creates the EHRs and commits the compositions an aql-conformance case arranges, in that order. The outputs name each EHR and the version uid of each COMPOSITION.',
          'x-veredictum-cite': {'source': 'specs/openehr/ITS-REST/specifications/operations/composition_create.yaml', 'section': 'description',
                                'quote': 'Creates the first version of a new COMPOSITION in the EHR identified by `ehr_id`.'}}
    if deps: wf['dependsOn'] = deps
    wf['inputs'] = {'type': 'object', 'properties': inputs}
    wf['steps'] = steps; wf['outputs'] = outputs
    seeds[sig] = (name, wf)
    return name

def setup_ref(seed, out): return f'$workflows.$sourceDescriptions.setup.{seed}.outputs.{out}'

BIND = re.compile(r"'?\$\{(\w+)\.(\w+)\}'?")
def bind_name(alias, field):
    if field == 'ehr_id': return 'ehr_id' if alias == 'ehr1' else alias + '_id'
    if field == 'status_uid': return 'status_uid'
    if field == 'uid': return alias
    raise SystemExit((alias, field))

def wf_id(cid):
    return cid.split('.', 1)[1].replace('.', '-')

docs = {}
def add(doc, wf): docs.setdefault(doc, []).append(wf)

def carried(cid, doc, ckey):
    path, d = cases[cid]
    seed = seed_for(d)
    act = d['act']; a = d['assert']
    used = []
    def sub(m):
        nm = bind_name(m.group(1), m.group(2)); used.append(nm); return '$' + nm
    q = BIND.sub(sub, act['query'])
    params = dict(act.get('parameters') or {})
    reps = []
    for nm in dict.fromkeys(used):
        params[nm] = f'replaced with the {nm} of the set-up'
        reps.append({'target': f'/query_parameters/{nm}', 'value': setup_ref(seed, nm)})
    payload = {'q': Lit(q) if '\n' in q else q}
    if params: payload['query_parameters'] = params
    body = {'contentType': 'application/json', 'payload': payload}
    if reps: body['replacements'] = reps
    refused = a['outcome'] == 'refused'
    crit = []
    if refused:
        crit.append({'condition': '$statusCode == 400', 'x-veredictum-cite': {'source': BAD400[0], 'section': BAD400[1], 'quote': BAD400[2]}})
    else:
        crit.append({'condition': '$statusCode == 200', 'x-veredictum-cite': {'source': OK200[0], 'section': OK200[1], 'quote': OK200[2]}})
        chk = {'judge': 'result_set', 'context': '$response.body'}
        cols = a.get('columns') or []
        names = [col.get('name') for col in cols]
        if 'rows' in a:
            def cell(v):
                if isinstance(v, str):
                    m = re.fullmatch(r"\$\{(\w+)\.(\w+)\}", v)
                    if m: return setup_ref(seed, bind_name(m.group(1), m.group(2)))
                    assert not v.startswith('$'), v
                return v
            chk['match'] = 'ordered' if (a.get('compare') or {}).get('order_sensitive') else 'set'
            chk['rows'] = [[cell(v) for v in r] for r in a['rows']]
        elif 'min_row_count' in a:
            chk['match'] = 'count'; chk['count'] = a['min_row_count']
        else:
            chk['match'] = 'count'; chk['count'] = a['row_count']
        if cols and all(names): chk['columns'] = names
        crit.append({'condition': '$statusCode == 200', 'x-veredictum-check': chk})
        if ckey == 'node_at':
            crit[-1]['x-veredictum-cite'] = {'source': RM_ROOT[0], 'section': RM_ROOT[1], 'quote': RM_ROOT[2]}
        if cols and any(names) and not all(names):
            for nm in names:
                if nm:
                    crit.append({'context': '$response.body', 'type': 'jsonpath', 'condition': f"$.columns[?@.name == '{nm}']",
                                 'x-veredictum-cite': cite('name_alias')})
    step = {'stepId': 'query',
            'description': (d['title'] + '.') if refused else 'Execute the query with POST, the identifiers of the set-up passed as query parameters.',
            'operationId': '$sourceDescriptions.query.query_execute_adhoc_query_body',
            'parameters': [{'name': 'Accept', 'in': 'header', 'value': 'application/json'}],
            'requestBody': body, 'successCriteria': crit}
    expect = 'is refused with 400' if refused else 'returns the rows the case expects'
    wf = {'workflowId': wf_id(cid), 'summary': d['title'],
          'description': f"Carries aql-conformance case {cid} (level {d['level']}, {path}): over the set-up {seed}, the query {expect}.",
          'dependsOn': [f'$sourceDescriptions.setup.{seed}'],
          'x-veredictum-cite': cite(ckey), 'steps': [step]}
    add(doc, wf)
    return f'{doc}/{wf_id(cid)}'

idmap = {}
for cid in sorted(T):
    kind = T[cid][0]
    lvl = cases[cid][1]['level']
    if kind == 'carry':
        _, doc, ck = T[cid]
        if doc == 'aql_ehr_scope':
            idmap[cid] = {'new': ['aql_ehr_scope/scope-composition_of_another_ehr']}; continue
        idmap[cid] = {'new': [carried(cid, doc, ck)]}
    elif kind == 'fold':
        e = {'new': T[cid][1]}
        if T[cid][2]: e['open'] = T[cid][2]
        idmap[cid] = e
    else:
        keys = T[cid][1]
        text = ' '.join(REASONS[k] for k in keys).replace('#FOLLOWUP', '#' + FOLLOWUP)
        if lvl == 'sensible' and not any(k in ISSUES or k == 'ext' for k in keys):
            raise SystemExit(f'sensible {cid} has no upstream report')
        idmap[cid] = {'new': [], 'open': f'Not carried (level {lvl}). ' + text}

# the ehr scope case is authored by hand; record its seed
scope_seed = seed_for(cases['envelope.ehr_scope.a_scoped_query_answers_nothing_from_another_ehr'][1])

tpl_wfs = []
for f, wid in TEMPLATES.items():
    xml = open(FIX + 'templates/' + f, encoding='utf-8').read()
    xml = '\n'.join(l.rstrip() for l in xml.replace('\r\n', '\n').strip('﻿').splitlines()).strip() + '\n'
    tid = re.search(r'<template_id>\s*<value>([^<]+)', xml).group(1)
    tpl_wfs.append({'workflowId': f'upload_template-{wid}', 'summary': f'The template {tid} is on the server',
        'description': f'Uploads the operational template {tid} from the aql-conformance fixture templates/{f}. A server that already holds a template with that template_id refuses the upload with 409, which leaves the same precondition in place.',
        'x-veredictum-cite': {'source': 'specs/openehr/ITS-REST/specifications/operations/definition_template_adl1.4_upload.yaml', 'section': 'description', 'quote': 'Upload a new ADL 1.4 operational template (OPT).'},
        'steps': [{'stepId': 'upload', 'description': 'Upload the template, or find it already there.',
                   'operationId': '$sourceDescriptions.definition.definition_template_adl1.4_upload',
                   'parameters': [{'name': 'Content-Type', 'in': 'header', 'value': 'application/xml'}],
                   'requestBody': {'contentType': 'application/xml', 'payload': Lit(xml)},
                   'successCriteria': [{'context': '$statusCode', 'type': 'regex', 'condition': '^(201|409)$',
                        'x-veredictum-cite': {'source': 'specs/openehr/ITS-REST/specifications/responses/409_template_already_exists.yaml', 'section': 'description', 'quote': '`409 Conflict` is returned when a template with same `template_id` already exists.'}}]}]})

SETUP_HEAD = f"""# The fixtures of the AQL conformance corpus (github.com/Cadasto/aql-conformance at
# {COMMIT}): its operational templates, and one set-up per arrangement
# a carried case makes, each an EHR with the case's COMPOSITIONs committed in
# order. Third-party bytes; aql_conformance.PROVENANCE.md beside this file and
# REUSE.toml name their source and licences. Every quote is copied verbatim from
# the named vendored file.
"""
setup = {'arazzo': '1.1.0', 'info': {'title': 'AQL conformance corpus fixtures', 'version': '0.1.0',
          'description': 'The templates and the per-case EHRs the carried aql-conformance cases query.'},
         'sourceDescriptions': [{'name': 'definition', 'url': '../../../specs/rest-oas/definition-validation.openapi.yaml', 'type': 'openapi'},
                                {'name': 'ehr', 'url': '../../../specs/rest-oas/ehr-validation.openapi.yaml', 'type': 'openapi'}],
         'workflows': tpl_wfs + [w for _, w in seeds.values()]}
open(REPO + '/catalogue/cases/_setup/aql_conformance.arazzo.yaml', 'w').write(SETUP_HEAD + dump(setup))

TITLES = {'aqlc_from': ('AQL conformance: FROM', 'The FROM-clause cases of the aql-conformance corpus whose answer the released AQL text assigns.'),
          'aqlc_select': ('AQL conformance: SELECT', 'The SELECT-clause cases of the aql-conformance corpus whose answer the released AQL text assigns.'),
          'aqlc_where': ('AQL conformance: WHERE', 'The WHERE-clause cases of the aql-conformance corpus whose answer the released AQL text assigns.'),
          'aqlc_aggregation': ('AQL conformance: aggregation', 'The aggregate-function cases of the aql-conformance corpus whose answer the released AQL text assigns.'),
          'aqlc_refused': ('AQL conformance: refusals', 'The aql-conformance queries the released AQL grammar or prose makes invalid, each refused with 400.'),
          'aqlc_top': ('AQL conformance: TOP', 'The deprecated TOP modifier, on the arm of the aql-top option that serves it.'),
          'aqlc_terminology': ('AQL conformance: TERMINOLOGY', 'A TERMINOLOGY call inside a matches value list, on the arm of the aql-terminology option that serves it.')}
for doc, wfs in docs.items():
    title, desc = TITLES[doc]
    head = f"""# {desc} Each workflow carries one case of the AQL conformance corpus
# (github.com/Cadasto/aql-conformance at {COMMIT[:12]}), named in its
# description; catalogue/id-map.yaml maps every case of the corpus. The case's
# own data is committed by the set-up it names in
# _setup/aql_conformance.arazzo.yaml, and its runtime identifiers reach the
# query as query parameters. Every quote is copied verbatim from the named
# vendored file.
"""
    d = {'arazzo': '1.1.0', 'info': {'title': title, 'version': '0.1.0', 'description': desc},
         'sourceDescriptions': [{'name': 'query', 'url': '../../../specs/rest-oas/query-validation.openapi.yaml', 'type': 'openapi'},
                                {'name': 'setup', 'url': '../_setup/aql_conformance.arazzo.yaml', 'type': 'arazzo'}],
         'workflows': wfs}
    open(f'{REPO}/catalogue/cases/query/{doc}.arazzo.yaml', 'w').write(head + dump(d))

json.dump({'idmap': idmap, 'scope_seed': scope_seed}, open(os.path.dirname(__file__) + '/out.json', 'w'), indent=1)
from collections import Counter
print(Counter(T[k][0] for k in T), {k: len(v) for k, v in docs.items()}, 'seeds', len(seeds))
