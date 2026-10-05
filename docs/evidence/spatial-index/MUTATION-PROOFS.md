# Mutation proofs — spatial index and hit testing (issue 0012)

Every check introduced with the index and hit testing, mutated at its source,
with the test that caught it. ED-0041 and ED-0053: a check never shown to fail
is not evidence of anything.

**Fourteen mutations, fourteen caught** — but four of them only after the tests
were rewritten, and that is the part worth reading.

| # | Check | Mutation | Test |
|---:|---|---|---|
| 1 | query filters candidates by actual intersection | drop the filter | `a_query_agrees_with_a_naive_scan` |
| 2 | proximity respects the radius | drop the filter | `a_proximity_query_agrees_with_a_naive_scan` |
| 3 | cell size is the MEDIAN extent | median → mean | `one_huge_entry_does_not_coarsen_the_grid_for_the_rest` |
| 4 | a multi-cell entry is deduplicated | drop the `seen` guard | `an_entry_spanning_many_cells_is_returned_once` |
| 5 | class precedence outranks distance | drop the class comparison | `a_join_node_outranks_the_regions_that_meet_there` |
| 6 | joined endpoints deduplicate to their join node | drop the skip | `a_joined_endpoint_never_appears_as_a_free_endpoint` |
| 7 | ties break by ascending entity id | drop the final comparison | `ranking_is_total_so_repeated_clicks_cycle_deterministically` |
| 8 | a point inside a region is at exact distance zero | return a boundary point instead | `a_click_inside_a_wall_hits_its_region_at_exact_distance_zero` |
| 9 | only active-level entities are eligible | iterate all walls | `walls_on_another_level_are_never_hit` |
| 10 | a zero-area marquee is a point hit | disable the branch | `a_zero_area_marquee_at_a_join_node_selects_the_join` |
| 11 | a zero-area marquee takes the TOP candidate only | return every candidate | `a_zero_area_marquee_at_a_join_node_selects_the_join` |
| 12 | marquee tests the polygon, not its bounding box | drop the polygon test | `a_marquee_inside_a_diagonal_walls_bounding_box_but_off_the_wall_misses` |
| 13 | marquee orders by marquee key | return `Equal` from the sort | `marquee_order_is_by_marquee_key_not_document_order` |
| 14 | endpoint membership uses a set, not a scan | (performance only — see below) | not asserted |

## The four tests that had to be rewritten first

Each of these passed before the mutation run and proved nothing. Finding them
is the entire reason the run happens.

**A result-only test cannot see the cell size (#3).** The original asserted
that a tight query near the origin returned at most three entries. But `query`
filters candidates by actual intersection whatever the grid looks like, so a
mean-sized cell — 25x too coarse, dragged out by one 1 km outlier — returns the
same ANSWER, only slower. The property is the cell size, so `cell_size()` is
now exposed for the test to assert on, documented as being for that and not for
callers to depend on.

**A bounding box that already rejects proves nothing about the polygon test
(#12).** The original marquee-miss test used axis-aligned walls, whose boxes are
tight; the box alone rejected, and removing the polygon test changed nothing.
The rewrite uses a DIAGONAL wall, whose box covers most of the square it spans
while the wall is a thin band through the middle. It also asserts that the same
rectangle moved onto the wall DOES select it, so the test cannot pass by
matching nothing.

**A test that asserted only "something was selected" could not see WHICH (#10,
#11).** The original zero-area marquee test asserted a point off the wall
selects nothing and a point on it selects the wall — both true through the
general intersection path, so disabling the point-hit branch entirely left it
green. The rewrite puts the query exactly on a JOIN NODE, where the two
readings differ observably: point-hit semantics return the join's identity,
intersection returns the two walls covering the node.

That test forced a specification question into the open, and the answer is
recorded in the source rather than in a commit message. R7.3.1 makes a
marquee's candidates wall REGIONS, which would make the branch redundant;
R7.3.2 says a zero-area rectangle takes POINT-HIT semantics, which are
R7.1.3's and rank join nodes above regions. Read as redundant the sentence says
nothing, so the second reading is taken — and "point-hit semantics" is read as
selecting what a CLICK selects, R7.2.2's top-ranked candidate, one entity.
Returning every candidate would make a zero-area drag select MORE than a click
on the same pixel.

## Row 14, which is not a correctness check

Endpoint membership moved from a `Vec::contains` scan to a `HashSet`. That
changes the build's complexity from quadratic in the number of joins to linear
and changes no answer, so no test can catch it and none pretends to. It is
listed so the row is not silently absent from a table that claims to cover
everything introduced.
