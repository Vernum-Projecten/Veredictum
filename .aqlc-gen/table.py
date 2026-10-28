# Classification of the 236 aql-conformance cases.
ISSUES = {'rowgrain': 701, 'subtype': 702, 'bareclass': 703, 'orrows': 704, 'refusals': 705,
          'nonlocatable': 706, 'aggregate': 707, 'latest': 708, 'ehrstatus_default': 709,
          'depth': 160, 'refs': 159, 'version': 168, 'multivalued_where': 195}

REASONS = {
 'ext': "Level ext:cadasto in the source: one vendor's own extension, which #695 keeps out of the catalogue.",
 'unpinned': "The query pins no EHR, COMPOSITION or EHR_STATUS of the case's own, so its answer depends on what else the server holds; ITS-REST docs/query/Request.md lets a backend use the ehr_id scope only \"to perform routing, optimizations or similar\".",
 'depth': "The answer needs CONTAINS to reach below a direct child; AQL never says whether containment is direct or at any depth, upstream report #160.",
 'refs': "Reaching an EHR_STATUS, a FOLDER or a VERSION from an EHR crosses a versioned-object reference, which AQL never defines as containment, upstream report #159.",
 'version': "A VERSION class operand has no released semantics, upstream report #168.",
 'multivalued_where': "A WHERE predicate over a multi-valued path; AQL never says how it evaluates, upstream report #195.",
 'rowgrain': "The rows a projection through a multi-valued attribute, an empty collection or no variable yields are not assigned, upstream report #701.",
 'subtype': "Whether a class expression naming an abstract RM class matches its descendants is not assigned, upstream report #702.",
 'bareclass': "A class expression with neither a variable nor a predicate breaks the prose rule its grammar admits, upstream report #703.",
 'orrows': "The rows of an OR between containment constraints, or what an unmatched or excluded variable reads, are not assigned, upstream report #704.",
 'refusals': "AQL assigns no outcome to this grammatical query, upstream report #705.",
 'nonlocatable': "Whether a class that is not LOCATABLE can be a FROM or CONTAINS operand is not assigned, upstream report #706.",
 'aggregate': "The group an aggregate function summarises is not defined, upstream report #707.",
 'latest': "Which version a query reads, and whether a logically deleted COMPOSITION is still contained, is not assigned, upstream report #708.",
 'ehrstatus_default': "The name and archetype_node_id of the default EHR_STATUS are not assigned, upstream report #709.",
 'lexer': "Its answer rests on the WS and COMMENT rules of AqlLexer.g4 alone, which no prose sentence states and the citation gate cannot quote yet; tracked by #FOLLOWUP.",
}

# id -> ('not', [reason keys]) | ('fold', [workflow ids], note) | ('carry', doc, cite key)
T = {}
def n(i, *keys): T[i] = ('not', list(keys))
def f(i, targets, note=None): T[i] = ('fold', targets, note)
def c(i, doc, cite): T[i] = ('carry', doc, cite)

S = 'aql_semantics/'
# FROM
n('FROM.abstract_type.abstract_entry_class_in_contains', 'subtype')
n('FROM.contains_depth.an_unmatched_anchor_removes_the_row_its_parent_projects', 'depth')
f('FROM.archetype_predicate.archetype_in_no_ready_template_is_empty_not_an_error', [S+'archetype_predicate-no_match'])
c('FROM.name_predicate.archetype_and_name_pair_predicate_filters_on_the_name', 'aqlc_from', 'node_name')
f('FROM.archetype_predicate.observation_in_composition', [S+'archetype_predicate'])
n('FROM.contains_depth.an_unpredicated_contained_element_unpinned', 'depth', 'unpinned')
c('FROM.contains_depth.composition_under_ehr', 'aqlc_from', 'class_ehr')
for x in ['deleted_composition_is_not_contained','deleted_composition_is_not_counted','deleting_one_of_two_leaves_the_other','deleting_one_of_two_leaves_the_other_without_the_ehr_scope','contribution_deletion_is_not_contained']:
    n('FROM.contains_depth.'+x, 'latest')
n('FROM.contains_depth.entry_under_unnamed_section', 'depth')
n('FROM.contains_depth.cluster_chain_across_compositions_of_one_ehr', 'depth')
n('FROM.contains_depth.type_only_entry_across_compositions_of_two_templates', 'depth')
n('FROM.node_predicate.an_anonymous_node_predicated_element_below_an_absent_archetype_is_empty', 'depth')
n('FROM.root_type.an_anonymous_predicated_root_class_binds_without_an_alias', 'unpinned', 'depth')
n('FROM.contains_depth.an_unaliased_trailing_class_constrains_its_parent', 'bareclass')
n('FROM.root_type.an_unaliased_unpredicated_root_anchors_a_chain', 'bareclass')
n('FROM.contains_boolean.an_and_junction_below_an_unaliased_section_step', 'bareclass')
n('FROM.contains_boolean.mixed_and_or_nesting_with_a_chain_inside_an_operand', 'orrows')
n('FROM.contains_boolean.two_containment_chains_joined_by_or', 'orrows')
n('FROM.archetype_predicate.whitespace_before_the_class_predicate_is_insignificant', 'unpinned')
n('FROM.contains_depth.data_structure_contained_by_composition_expands', 'subtype', 'depth')
n('FROM.contains_depth.an_unpredicated_contained_element_answers_every_occurrence', 'depth')
n('FROM.contains_depth.item_structure_contained_by_composition_expands', 'subtype', 'depth')
for x in ['a_path_into_other_details_the_status_does_not_hold_is_null','the_ehr_status_part_path_agrees_with_the_contained_spelling','the_part_spelling_of_the_whole_subject_agrees','a_where_clause_on_ehr_id_scopes_the_contained_status','the_whole_subject_object_is_projected','flags_and_subject','version_uid']:
    n('structure.ehr_status.'+x, 'refs')
n('FROM.contains_depth.element_contained_by_an_ehr_root_reaches_its_compositions', 'depth')
n('FROM.contains_depth.lower_case_contains_binds_like_the_upper_case_spelling', 'depth')
n('FROM.contains_depth.lower_case_contains_across_a_nested_cluster_chain', 'depth')
for x in ['activity_root_answers_empty','admin_entry_is_granted_and_answers_empty','cluster_is_a_granted_structural_root','evaluation_is_granted_and_answers_empty','history_is_a_granted_root_at_the_event_container_grain','interval_event_is_granted_and_answers_empty','item_tree_is_a_granted_concrete_structural_root','literal_only_projection_agrees_with_the_path_spelling','entry_root_with_archetype_predicate','type_only_structural_root_binds_every_occurrence']:
    n('FROM.root_type.'+x, 'unpinned')
for x in ['care_entry_expands_to_its_concrete_subtypes','data_structure_root_expands_to_every_data_structure','item_structure_is_a_granted_structural_root','abstract_event_root_binds_abstractly_declared_nodes','concrete_subtype_root_narrows_an_abstractly_declared_node']:
    n('FROM.root_type.'+x, 'unpinned', 'subtype')
n('FROM.abstract_type.abstract_entry_class_as_root', 'unpinned', 'subtype')
for x in ['feeder_audit_root_reads_what_the_ehr_holds','instruction_details_root_answers_empty','event_context_containment_refused','event_context_root','extract_root_answers_empty']:
    n('FROM.root_type.'+x, 'nonlocatable')
n('FROM.contains_depth.a_cluster_contained_by_an_unaliased_cluster', 'bareclass')
n('FROM.contains_depth.a_section_contained_by_an_unaliased_section', 'bareclass')
n('FROM.node_predicate.an_at_code_on_a_contained_element_matches_its_archetype_node_id', 'depth')
c('FROM.contains_depth.entry_rm_type_without_archetype_predicate', 'aqlc_from', 'class_parts')
for x in ['entry_descent_below_a_general_root','entry_descent_below_a_general_root_unpinned','type_only_structural_descent_below_a_general_root','type_only_structural_descent_below_a_general_root_unpinned','shorthand_chain_equals_the_expanded_chain','outer_anchor_leaf_repeats_across_inner_rows','a_nested_anchor_that_nothing_reads_still_restricts_the_rows','nested_anchor_excludes_its_own_parent']:
    n('FROM.contains_depth.'+x, 'depth')
n('FROM.contains_depth.lower_case_contains_unpinned', 'depth', 'unpinned')
n('FROM.root_type.literal_only_projection_keeps_the_entry_roots_occurrence_grain', 'rowgrain', 'unpinned')
f('FROM.contains_boolean.negation_admits_a_composition_lacking_the_archetype', [S+'contains-not'])
f('FROM.contains_boolean.negation_excludes_a_composition_that_has_the_archetype', [S+'contains-not'])
c('FROM.node_predicate.an_at_code_on_an_archetype_root_class_matches_nothing', 'aqlc_from', 'node_at')
n('FROM.contains_boolean.not_contains_excludes_a_composition_holding_the_negated_chain', 'depth')
c('FROM.contains_boolean.not_contains_at_the_ehr_root_excludes_an_ehr_holding_the_archetype', 'aqlc_from', 'not_contains')
n('FROM.contains_boolean.not_contains_below_an_entry_excludes_an_entry_holding_the_class', 'depth')
n('FROM.archetype_predicate.opt_pk_intersection_narrows_the_candidates', 'depth')
n('FROM.archetype_predicate.opt_pk_union_across_two_templates', 'depth')
f('FROM.contains_boolean.or_containment_answers_the_operand_that_matches', [S+'contains-or'],
  "Re-grounded on QUERY master03-syntax.adoc §OR: with one operand matching nothing, the OR constraint holds exactly for the parents that contain the other.")
c('FROM.archetype_predicate.parameterised_archetype_predicate_binds_like_the_archetype_id', 'aqlc_from', 'param_pred')
n('FROM.contains_boolean.a_path_read_from_a_negated_alias_is_null', 'orrows')
c('FROM.contains_depth.alias_declared_twice_is_invalid', 'aqlc_refused', 'var_unique')
n('FROM.root_type.unknown_rm_class_root_refused', 'refusals')
n('FROM.root_type.folder_root', 'refs', 'unpinned')
n('FROM.root_type.folder_root_over_a_version', 'refs', 'version')
c('FROM.root_type.composition_root', 'aqlc_from', 'from_subset')
c('FROM.root_type.composition_root_unpinned', 'aqlc_from', 'limit')
c('FROM.root_type.ehr_status_root', 'aqlc_from', 'ehr_create_default')
n('FROM.root_type.version_root_reads_the_version_of_a_composition', 'version')
c('FROM.standing_predicate.non_uuid_scope_yields_empty_result', 'aqlc_from', 'count_zero')
c('FROM.archetype_predicate.predicated_section_anchor_above_the_entry', 'aqlc_from', 'contains_parent')
n('FROM.node_predicate.nested_anchor_narrowed_by_at_code', 'depth')
f('FROM.standing_predicate.ehr_id_equivalent_to_out_of_band_scope', [S+'standard_predicate-ehr_id'])
c('FROM.contains_boolean.two_bound_siblings_answer_no_row_where_one_matches_nothing', 'aqlc_from', 'and_def')
c('FROM.contains_depth.an_unpredicated_section_alias_binds_above_the_entry', 'aqlc_from', 'contains_parent')
# ORDER_BY
n('ORDER_BY.array_valued_path.ordering_by_an_array_path_sorts_its_element_rows', 'rowgrain')
f('ORDER_BY.single.content_path_descending', [S+'order_by-descending'])
n('ORDER_BY.array_valued_path.ordering_by_an_unprojected_mappings_path_keeps_one_row', 'rowgrain')
n('ORDER_BY.single.order_by_a_projection_alias_executes', 'ext')
n('ORDER_BY.array_valued_path.ordering_by_a_collection_node_is_refused', 'refusals')
# SELECT
n('SELECT.identified_path.absent_node_yields_a_null_cell_not_a_dropped_row', 'depth')
f('SELECT.alias.named_column', [S+'column-alias_name'])
n('SELECT.identified_path.predicate_on_the_alias_root_reads_the_object_that_satisfies_it', 'refusals')
for x in ['an_absent_links_collection_keeps_its_row','an_empty_collection_beside_a_bound_occurrence_keeps_its_row','the_collection_node_stays_an_object_on_the_content_tier','the_collection_node_unnests','an_empty_array_collection_keeps_its_row','collection_scope_binding_refused','an_explicit_null_collection_keeps_its_row','elements_open_a_row_scope','two_empty_independent_collections_keep_their_row','an_absent_mappings_key_keeps_its_row','the_mappings_collection_node_unnests','an_empty_mappings_array_keeps_its_row','an_explicit_null_mappings_keeps_its_row','mappings_over_a_joined_document','mappings_elements_open_a_row_scope','mappings_under_a_dv_text_subtype']:
    n('SELECT.array_valued_path.'+x, 'rowgrain')
n('SELECT.array_valued_path.a_collection_nested_in_an_empty_one_keeps_its_row', 'rowgrain', 'nonlocatable')
n('SELECT.array_valued_path.mappings_under_an_event_context_root', 'rowgrain', 'nonlocatable')
n('SELECT.array_valued_path.an_unprojected_array_path_in_where_filters_without_multiplying', 'multivalued_where')
n('SELECT.array_valued_path.an_unprojected_array_path_in_where_matching_no_element', 'multivalued_where')
n('SELECT.array_valued_path.an_unprojected_mappings_path_in_where_filters_without_multiplying', 'multivalued_where')
c('SELECT.identified_path.a_content_path_matching_no_node_is_a_null_cell', 'aqlc_select', 'result_null')
n('SELECT.identified_path.a_path_through_content_reads_each_content_item', 'rowgrain')
n('SELECT.identified_path.content_text_leaf_exact_value', 'depth')
c('SELECT.identified_path.content_typed_numeric_leaves', 'aqlc_select', 'select_path')
f('SELECT.distinct.statement_distinct_removes_duplicate_rows', [S+'distinct'])
c('SELECT.identified_path.ehr_id_scalar', 'aqlc_select', 'rm_attr_path')
n('SELECT.identified_path.ehr_root_alias_beside_a_deep_ehr_status_document_path', 'refs')
c('SELECT.identified_path.ehr_root_alias_repeats_over_multiple_contained_rows', 'aqlc_select', 'class_ehr')
n('SELECT.identified_path.ehr_root_system_id_joins_beside_a_contained_class', 'refs')
n('SELECT.identified_path.event_context_fixed_attributes', 'nonlocatable')
n('SELECT.identified_path.duplicate_result_column_is_invalid', 'refusals')
n('SELECT.identified_path.the_character_match_of_each_term_mapping', 'rowgrain')
c('SELECT.identified_path.composition_metadata_paths', 'aqlc_select', 'rm_attr_path')
n('SELECT.identified_path.ehr_status_document_paths_resolve', 'ehrstatus_default')
n('SELECT.identified_path.event_context_sub_paths_resolve', 'nonlocatable')
n('SELECT.identified_path.interval_bound_flags_resolve', 'ext')
c('SELECT.identified_path.rm_defined_metadata_paths_resolve', 'aqlc_select', 'rm_attr_path')
c('SELECT.identified_path.system_id_registry_join', 'aqlc_select', 'rm_attr_path')
n('SELECT.identified_path.alias_rename_does_not_change_the_answer', 'depth')
n('SELECT.identified_path.ehr_root_alias_projects_beside_a_contained_composition', 'refs')
n('SELECT.identified_path.ehr_root_alias_projects_beside_a_contained_ehr_status', 'refs')
n('SELECT.identified_path.a_path_without_a_node_predicate_reads_the_element_of_each_cluster', 'depth', 'rowgrain')
f('SELECT.positional_column_name.two_paths', [S+'column-index_names'])
c('SELECT.primitive.literal_with_an_as_alias_takes_the_alias', 'aqlc_select', 'name_alias')
f('SELECT.primitive.every_literal_kind_projects_its_value', [S+'literal-string', S+'literal-integer', S+'literal-real', S+'literal-boolean', S+'literal-null'])
c('SELECT.primitive.literal_mixed_with_an_identified_path', 'aqlc_select', 'literal')
c('SELECT.primitive.literal_mixed_with_a_whole_object_and_a_path', 'aqlc_select', 'column_expr')
c('SELECT.primitive.primitive_literal_projection_refused', 'aqlc_select', 'literal')
c('SELECT.primitive.literal_repeats_over_the_filtered_rows', 'aqlc_select', 'literal')
n('SELECT.identified_path.metadata_path_the_reference_model_does_not_define_refused', 'refusals')
n('SELECT.identified_path.node_predicate_on_a_metadata_segment_refused', 'refusals')
n('SELECT.whole_object.order_by_a_whole_object_column_refused', 'refusals')
n('SELECT.identified_path.unresolvable_root_alias_path_names_the_root_class', 'refusals')
c('SELECT.identified_path.select_star_refused', 'aqlc_refused', 'column_expr')
c('SELECT.identified_path.select_star_mixed_with_a_column_refused', 'aqlc_refused', 'column_expr')
c('SELECT.paging.select_top_limits_the_rows', 'aqlc_top', 'top')
n('SELECT.identified_path.trivia_inside_node_predicates_is_not_part_of_the_predicate', 'lexer')
c('SELECT.whole_object.composition_alias', 'aqlc_select', 'select_var')
for x in ['ehr_alias_refused','ehr_root_through_the_content_tier','ehr_alias_refused_beside_a_contained_class']:
    n('SELECT.whole_object.'+x, 'ext')
c('SELECT.whole_object.ehr_status_alias', 'aqlc_select', 'select_var')
c('SELECT.whole_object.entry_alias', 'aqlc_select', 'select_var')
n('SELECT.whole_object.event_context_alias', 'nonlocatable')
c('SELECT.whole_object.a_path_ending_at_an_object_projects_it_whole', 'aqlc_select', 'select_path')
c('SELECT.identified_path.reference_to_an_undeclared_alias_is_invalid', 'aqlc_refused', 'var_from')
# WHERE
for x in ['two_terms_on_one_collection_bind_the_same_element','two_terms_on_one_collection_matched_by_one_element','two_terms_on_one_mappings_collection_bind_the_same_element','two_terms_on_one_mappings_collection_matched_by_one_element','two_unprojected_mappings_arrays_filter_independently']:
    n('WHERE.comparison.'+x, 'multivalued_where')
n('WHERE.boolean.boolean_literal_comparison_executes', 'refs')
c('WHERE.comparison.template_id_no_match', 'aqlc_where', 'equal')
f('WHERE.comparison.less_or_equal_holds_at_the_boundary_and_below_it', [S+'where-less_or_equal'])
c('WHERE.comparison.template_id_equals', 'aqlc_where', 'equal')
f('WHERE.comparison.typed_numeric_inside_the_bound_occurrence', [S+'where-greater_or_equal'])
c('WHERE.comparison.a_date_time_without_an_offset_compares_with_one_that_has_it', 'aqlc_where', 'dates_note')
f('WHERE.exists.a_path_that_exists_keeps_its_row', [S+'where-exists'])
c('WHERE.comparison.a_function_call_as_the_left_operand_filters', 'aqlc_where', 'left_function')
f('WHERE.comparison.a_function_call_as_the_right_operand_filters', [S+'function-now_in_where'])
f('WHERE.like.like_matches_with_its_wildcards', [S+'like-question_mark', S+'like-star'])
c('WHERE.matches.a_terminology_call_merges_into_a_value_list', 'aqlc_terminology', 'terminology')
f('WHERE.matches.matches_a_value_in_its_list', [S+'where-matches'])
f('WHERE.boolean.not_negates_a_comparison', [S+'where-not'])
c('WHERE.null_comparison.a_false_conjunct_beside_a_null_comparison_matches_nothing', 'aqlc_where', 'and_def')
f('WHERE.boolean.or_keeps_a_row_either_comparison_matches', [S+'where-or'])
c('WHERE.comparison.a_partial_date_compares_with_a_complete_date_time', 'aqlc_where', 'dates_partial')
n('WHERE.comparison.a_text_value_never_equals_a_number', 'refusals')
c('WHERE.comparison.ehr_root_alias_projects_through_a_filtered_query', 'aqlc_where', 'class_ehr')
f('WHERE.comparison.tier_interplay_on_a_metadata_shape', [S+'where-not_equal'])
# aggregation
f('aggregation.sum_avg.avg_is_the_mean_of_the_non_null_values', [S+'aggregate-avg'])
n('aggregation.count.count_beside_a_column_counts_each_value_of_the_column', 'aggregate')
n('aggregation.count.composition_count_is_unaffected_by_occurrences', 'aggregate', 'depth')
n('aggregation.count.composition_versus_occurrence', 'depth')
f('aggregation.count_distinct.counts_each_distinct_value_once', [S+'aggregate-count_distinct'])
c('aggregation.count.count_over_a_path_counts_the_values_present', 'aqlc_aggregation', 'agg_null')
f('aggregation.min_max.max_returns_the_largest_value', [S+'aggregate-max'])
f('aggregation.min_max.min_returns_the_smallest_non_null_value', [S+'aggregate-min'])
n('aggregation.min_max.min_over_a_string_returns_the_lowest_string', 'depth')
f('aggregation.sum_avg.sum_adds_the_non_null_values', [S+'aggregate-sum'])
f('aggregation.count.composition_count', [S+'aggregate-count'])
# envelope
c('envelope.ehr_scope.a_scoped_query_answers_nothing_from_another_ehr', 'aql_ehr_scope', 'ehr_scope')
n('envelope.result_set_meta.row_per_entry_occurrence', 'depth')
n('envelope.result_set_meta.singleton_siblings_do_not_multiply_rows', 'depth')
f('envelope.stored_query.parity_with_adhoc', ['aql_stored/post-latest'])
# functions
for k, w in [('date_time.current_date_has_the_yyyy_mm_dd_format','current_date'),('date_time.current_date_time_has_the_extended_date_time_format','current_date_time'),('date_time.current_time_has_the_hh_mm_ss_format','current_time'),('date_time.current_timezone_has_the_hh_mm_offset_format','current_timezone'),('date_time.now_has_the_extended_date_time_format','now'),('numeric.abs_turns_a_negative_value_positive_and_keeps_a_positive_one','abs'),('numeric.ceil_is_the_integer_at_or_above_its_argument','ceil'),('numeric.floor_is_the_integer_at_or_below_its_argument','floor'),('numeric.mod_is_the_remainder_of_the_first_argument_divided_by_the_second','mod'),('numeric.round_rounds_to_the_given_places_and_to_a_whole_number_without_them','round'),('string.concat_joins_its_arguments_in_order','concat'),('string.concat_ws_puts_the_separator_between_its_arguments','concat_ws'),('string.substring_takes_length_characters_from_a_position_counted_from_one','substring'),('string.substring_without_a_length_runs_to_the_end','substring_no_length')]:
    f('functions.'+k, [S+'function-'+w])
f('functions.string.position_counts_from_one_and_is_zero_when_absent', [S+'function-position', S+'function-position_absent'])
# paging
f('paging.limit.bounds_row_count', ['aql_execute/get-fetch'])
f('paging.offset.limit_and_offset_partition_an_ordered_set', [S+'limit-offset'])
n('paging.limit.bounds_collection_element_rows', 'rowgrain')
f('paging.offset.shifts_window', ['aql_execute/get-offset_fetch'])
c('paging.limit.parameterised_limit_refused', 'aqlc_refused', 'limit_int')
# parameters
f('parameters.in_where.template_id', [S+'parameter-string'])
f('parameters.typed_literal.equals_bound_parameter', ['aqlc_where/comparison-template_id_equals'])
# structure / version
n('structure.folder.folder_containment', 'refs')
n('structure.folder.version_under_a_folder', 'refs', 'version')
n('structure.folder.composition_version_under_a_folder', 'refs', 'version')
n('version.all_versions.version_containment_reads_the_version_of_each_composition', 'version')
