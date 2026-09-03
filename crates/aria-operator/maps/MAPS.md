# 25 sealed market-map mixers

Sheet `05_MAP_COVERAGE_25` + `13_MAP_LANGUAGE_25`. These are **refinement**
operators (`BIN.REF.*`). They run on already-tagged JSON telemetry
(a dump callback, a worker `aria-work-v1`, or a raw graph that already
carries kinds/rels/tags). They do **not** invent entities, scores, or Trust.
The original dump remains the evidence set; each mixer is a standalone
map-shaped slice of that same data.

Viral coefficient: one processed payload × 25 mixers = 25 structured map JSON
results. Mode 2 enrichment reads these envelopes to draw the graphic.
The node never scores.

| # | Map | `BIN.REF.*` | intent | family | primary binaries | rels |
|---|---|---|---|---|---|---|
| 1 | Competitive Radar | `BIN.REF.COMPETITIVE_RADAR` | compare | quadrant | COMPANY, COMPETITOR, SIGNAL, CLAIM | COMPETES_WITH |
| 2 | Category Galaxy | `BIN.REF.CATEGORY_GALAXY` | understand | network | COMPANY, MARKET, SIGNAL | IN_CATEGORY, ADJACENT_TO |
| 3 | Market Heat Island | `BIN.REF.MARKET_HEAT_ISLAND` | understand | quadrant | COMPANY, SIGNAL, EVENT, MARKET | SIGNALS, OPERATES_IN |
| 4 | Buyer Persona Relationship | `BIN.REF.BUYER_PERSONA_RELATIONSHIP` | compare | network | PEOPLE, COMPANY, BUYER, CLAIM | WORKS_AT, BUYS, HAS_PERSONA |
| 5 | Investor Syndicate | `BIN.REF.INVESTOR_SYNDICATE` | capital | hub | VENTURE_CAPITAL, COMPANY, SYNDICATE | INVESTS_IN, CO_INVESTS_WITH, MEMBER_OF |
| 6 | Customer Logos | `BIN.REF.CUSTOMER_LOGOS` | output | swimlane | CUSTOMER, COMPANY, SELLER, CLAIM | CUSTOMER_OF, SELLS_TO |
| 7 | Developer Ecosystem | `BIN.REF.DEVELOPER_ECOSYSTEM` | opportunity | network | COMPANY, PRODUCT, PARTNER, PEOPLE | INTEGRATES_WITH, OFFERED_BY |
| 8 | Partnership Path | `BIN.REF.PARTNERSHIP_PATH` | opportunity | network | COMPANY, PARTNER, ACCOUNT | PARTNERS_WITH, PATH_TO |
| 9 | Wedge-to-Platform | `BIN.REF.WEDGE_TO_PLATFORM` | opportunity | swimlane | COMPANY, PRODUCT, SIGNAL | OFFERED_BY, HAS_CAPABILITY |
| 10 | Market Maturity | `BIN.REF.MARKET_MATURITY` | understand | timeline | COMPANY, MARKET, EVENT, SIGNAL | OPERATES_IN, OCCURRED_AT |
| 11 | Competitor Battlecard | `BIN.REF.COMPETITOR_BATTLECARD` | compare | swimlane | COMPANY, PRODUCT, COMPETITOR, CLAIM | COMPETES_WITH, SUBSTITUTES, CITES |
| 12 | Funding Momentum | `BIN.REF.FUNDING_MOMENTUM` | capital | timeline | COMPANY, VENTURE_CAPITAL, SIGNAL, EVENT | INVESTS_IN, RAISES_FROM, SIGNALS |
| 13 | Influence Network | `BIN.REF.INFLUENCE_NETWORK` | opportunity | network | PEOPLE, COMPANY, VENTURE_CAPITAL | INFLUENCES, WORKS_AT, FOUNDED |
| 14 | Category Stack | `BIN.REF.CATEGORY_STACK` | understand | swimlane | COMPANY, PRODUCT, MARKET | IN_CATEGORY, OFFERED_BY |
| 15 | Market Map Canvas | `BIN.REF.MARKET_MAP_CANVAS` | understand | quadrant | COMPANY, MARKET, PORTFOLIO | IN_CATEGORY, PLACED_IN |
| 16 | ICP Expansion | `BIN.REF.ICP_EXPANSION` | opportunity | quadrant | ACCOUNT, PEOPLE, BUYER, CUSTOMER | HAS_PERSONA, ADJACENT_TO, BUYS |
| 17 | Market White Space | `BIN.REF.MARKET_WHITE_SPACE` | opportunity | quadrant | COMPANY, MARKET, SIGNAL | IN_CATEGORY, ADJACENT_TO |
| 18 | Account Targeting | `BIN.REF.ACCOUNT_TARGETING` | opportunity | swimlane | ACCOUNT, COMPANY, PEOPLE, PARTNER | TARGETED_BY, PATH_TO, HAS_PERSONA |
| 19 | Narrative Positioning | `BIN.REF.NARRATIVE_POSITIONING` | compare | quadrant | COMPANY, CLAIM, SOURCE, CONTENT | ASSERTS, CITES |
| 20 | Founder/Operator Lineage | `BIN.REF.FOUNDER_OPERATOR_LINEAGE` | opportunity | network | PEOPLE, COMPANY | FOUNDED, WORKS_AT, PREVIOUSLY_AT |
| 21 | Product Capability | `BIN.REF.PRODUCT_CAPABILITY` | compare | swimlane | PRODUCT, COMPANY, CLAIM | HAS_CAPABILITY, OFFERED_BY, SUBSTITUTES |
| 22 | Market Entry | `BIN.REF.MARKET_ENTRY` | opportunity | brief | MARKET, GEOGRAPHY, PARTNER, COMPANY | OPERATES_IN, PATH_TO, PARTNERS_WITH |
| 23 | Ecosystem Gravity | `BIN.REF.ECOSYSTEM_GRAVITY` | opportunity | hub | COMPANY, PARTNER, PRODUCT | PARTNERS_WITH, INTEGRATES_WITH |
| 24 | Category Creation Timeline | `BIN.REF.CATEGORY_CREATION_TIMELINE` | output | timeline | EVENT, COMPANY, SIGNAL, CONTENT | ANNOUNCED, OCCURRED_AT, SIGNALS |
| 25 | Market Intelligence Brief | `BIN.REF.MARKET_INTELLIGENCE_BRIEF` | output | brief | COMPANY, MARKET, CLAIM, SOURCE, PORTFOLIO | ASSERTS, CITES, CONTAINS_MAP |

## Per map

### 01. Competitive Radar

- **binary:** `BIN.REF.COMPETITIVE_RADAR`
- **use:** `work --binary BIN.REF.COMPETITIVE_RADAR` on a dump callback or tagged graph
- **intent / family / group:** compare / quadrant / LANDSCAPE
- **kinds:** Company, MarketSignal, Claim
- **rels:** COMPETES_WITH
- **cast rule (13):** Cast competitor + pressure + proof. Refuse coverage-count language as a differentiator.
- **language tags:** LANG_POSITION, LANG_PROOF, LANG_COMPETITIVE_PRESSURE, LANG_EVIDENCE_STRENGTH, LANG_DIFFERENTIATION, LANG_THREAT_CRITICAL, LANG_THREAT_HIGH, LANG_THREAT_MEDIUM, LANG_THREAT_LOW, LANG_COMPETE_PROGRAM, LANG_MATURE, LANG_COMPARISON_MATRIX…

Pressure, proof, differentiation, threat levels, compete-program — the radar can now type-cast incoming CI language instead of leaving it as free text.

### 02. Category Galaxy

- **binary:** `BIN.REF.CATEGORY_GALAXY`
- **use:** `work --binary BIN.REF.CATEGORY_GALAXY` on a dump callback or tagged graph
- **intent / family / group:** understand / network / NETWORK
- **kinds:** Company, Market, Category, MarketSignal
- **rels:** IN_CATEGORY, ADJACENT_TO
- **cast rule (13):** Cast neighborhood + distance. Refuse a 26th cluster name.
- **language tags:** LANG_CATEGORY_NEIGHBORHOOD, LANG_CLUSTER_GRAVITY, LANG_CROWDED_AREA, LANG_OPEN_AREA, LANG_CATEGORY_DISTANCE, LANG_CORE_CATEGORY_MEMBER, LANG_UNDERSERVED_CLUSTER, LANG_PULL, LANG_CATEGORY_FORMATION

Neighborhood, gravity, crowded/open, category-distance, bridge-band — galaxy placement language becomes tags.

### 03. Market Heat Island

- **binary:** `BIN.REF.MARKET_HEAT_ISLAND`
- **use:** `work --binary BIN.REF.MARKET_HEAT_ISLAND` on a dump callback or tagged graph
- **intent / family / group:** understand / quadrant / LANDSCAPE
- **kinds:** Company, MarketSignal, Event, Market, Category
- **rels:** SIGNALS, OPERATES_IN
- **cast rule (13):** Cast signal subtype into a heat tag. Refuse undated heat.
- **language tags:** LANG_HEAT_ZONE, LANG_FUNDING_HEAT, LANG_HIRING_HEAT, LANG_LAUNCH_HEAT, LANG_ATTENTION_HEAT, LANG_RECENCY_HEAT, LANG_EMERGING, LANG_GROWING, LANG_ROUND_SIZE, LANG_MOMENTUM_UP, LANG_MOMENTUM_FLAT, LANG_MOMENTUM_DOWN…

Heat by funding / hiring / launch / attention / recency — heat is no longer a single undifferentiated blob.

### 04. Buyer Persona Relationship

- **binary:** `BIN.REF.BUYER_PERSONA_RELATIONSHIP`
- **use:** `work --binary BIN.REF.BUYER_PERSONA_RELATIONSHIP` on a dump callback or tagged graph
- **intent / family / group:** compare / network / NETWORK
- **kinds:** Person, Company, Account, Claim
- **rels:** WORKS_AT, BUYS, HAS_PERSONA
- **cast rule (13):** Cast title → PERSON_* then persona → PERSONA_*. Both may apply.
- **language tags:** LANG_BUYER_ROLE, LANG_PROBLEM_POINT, LANG_PROOF_POINT, LANG_PERSONA_LINK, LANG_ADJACENT_BUYER

Full persona seat + archetype split (economic / technical / user / governance) plus MRD buyer titles.

### 05. Investor Syndicate

- **binary:** `BIN.REF.INVESTOR_SYNDICATE`
- **use:** `work --binary BIN.REF.INVESTOR_SYNDICATE` on a dump callback or tagged graph
- **intent / family / group:** capital / hub / INVESTOR
- **kinds:** Investor, Fund, Company, Syndicate
- **rels:** INVESTS_IN, CO_INVESTS_WITH, MEMBER_OF
- **cast rule (13):** Cast person-investor vs fund-vehicle separately (PERSON_GP vs CO_FUND_VEHICLE vs INVESTOR kind).
- **language tags:** LANG_CO_INVESTOR, LANG_PORTFOLIO_OVERLAP, LANG_CAPITAL_PATH, LANG_LEAD_INVESTOR, LANG_FOLLOW_ON, LANG_STRATEGIC_RING, LANG_ROUND_SIZE, LANG_INVESTOR_QUALITY, LANG_VINTAGE, LANG_GRAVITY_HUB

Lead / follow-on / overlap / strategic ring / GP-LP person types sit beside the INVESTOR entity.

### 06. Customer Logos

- **binary:** `BIN.REF.CUSTOMER_LOGOS`
- **use:** `work --binary BIN.REF.CUSTOMER_LOGOS` on a dump callback or tagged graph
- **intent / family / group:** output / swimlane / CANVAS
- **kinds:** Customer, Account, Company, Product, Claim
- **rels:** CUSTOMER_OF, SELLS_TO
- **cast rule (13):** Cast vendor/customer/seller/buyer. Relationship strength is a tag, not Trust.
- **language tags:** LANG_PROOF, LANG_PROOF_POINT, LANG_VENDOR, LANG_RELATIONSHIP_STRENGTH, LANG_LOGO_LANE, LANG_PROVEN_CUSTOMER_CONTEXT, LANG_PROOF_BACKED_STRENGTH

Vendor vs customer vs relationship-strength vs logo-lane. Seller/buyer type-cast into lanes.

### 07. Developer Ecosystem

- **binary:** `BIN.REF.DEVELOPER_ECOSYSTEM`
- **use:** `work --binary BIN.REF.DEVELOPER_ECOSYSTEM` on a dump callback or tagged graph
- **intent / family / group:** opportunity / network / NETWORK
- **kinds:** Company, Product, Person
- **rels:** INTEGRATES_WITH, OFFERED_BY
- **cast rule (13):** Cast builder/integrator/champion/MCP. Agent-callable is a product tag, not a View.
- **language tags:** LANG_BUILDER_NODE, LANG_PROJECT_NODE, LANG_INTEGRATION, LANG_ECOSYSTEM_CHAMPION, LANG_TALENT_LINEAGE, LANG_PULL, LANG_GRAVITY_HUB

Builder / project / integration / champion / MCP / agent-callable / SDK — ecosystem grammar.

### 08. Partnership Path

- **binary:** `BIN.REF.PARTNERSHIP_PATH`
- **use:** `work --binary BIN.REF.PARTNERSHIP_PATH` on a dump callback or tagged graph
- **intent / family / group:** opportunity / network / NETWORK
- **kinds:** Company, Account
- **rels:** PARTNERS_WITH, PATH_TO
- **cast rule (13):** Cast warm vs cold. Cold is a valid tag (gap), not a failure.
- **language tags:** LANG_STRATEGIC_RING, LANG_RELATIONSHIP_STRENGTH, LANG_STRATEGIC_TARGET, LANG_WARM_PATH_HOP, LANG_PARTNER_PATH, LANG_INFLUENCE_PATH, LANG_EXPANSION_PATH, LANG_WARM_PATH_ACCOUNT, LANG_OPERATOR_PATH, LANG_ENTRY_WEDGE, LANG_PARTNER_PATH_ENTRY

Strategic target, warm/cold path, SI / reseller / cloud partner. Path is typed.

### 09. Wedge-to-Platform

- **binary:** `BIN.REF.WEDGE_TO_PLATFORM`
- **use:** `work --binary BIN.REF.WEDGE_TO_PLATFORM` on a dump callback or tagged graph
- **intent / family / group:** opportunity / swimlane / CANVAS
- **kinds:** Company, Product, MarketSignal
- **rels:** OFFERED_BY, HAS_CAPABILITY
- **cast rule (13):** Cast layer tags before computing expansion. Layer is placement language.
- **language tags:** LANG_LAUNCH_HEAT, LANG_INITIAL_WEDGE, LANG_PLATFORM_THESIS, LANG_EXPANSION_STEP, LANG_LAYER_INFRA, LANG_LAYER_MID, LANG_LAYER_APP, LANG_LAYER_DIST, LANG_STACK_INFRA, LANG_STACK_MIDDLEWARE, LANG_STACK_APPLICATION, LANG_STACK_DISTRIBUTION…

Wedge, platform thesis, four stack layers as tags so expansion steps type-cast.

### 10. Market Maturity

- **binary:** `BIN.REF.MARKET_MATURITY`
- **use:** `work --binary BIN.REF.MARKET_MATURITY` on a dump callback or tagged graph
- **intent / family / group:** understand / timeline / LANDSCAPE
- **kinds:** Company, Market, Category, Event, MarketSignal
- **rels:** OPERATES_IN, OCCURRED_AT
- **cast rule (13):** Cast lifecycle tag. Public + consolidating may co-exist.
- **language tags:** LANG_EXPANSION_STEP, LANG_EMERGING, LANG_GROWING, LANG_CONSOLIDATING, LANG_MATURE, LANG_LIFECYCLE_CURVE, LANG_MARKET_RISK, LANG_TEMPORAL_SIGNAL

Emerging / growing / consolidating / mature plus acquirer / listed incumbent.

### 11. Competitor Battlecard

- **binary:** `BIN.REF.COMPETITOR_BATTLECARD`
- **use:** `work --binary BIN.REF.COMPETITOR_BATTLECARD` on a dump callback or tagged graph
- **intent / family / group:** compare / swimlane / CANVAS
- **kinds:** Company, Product, Claim
- **rels:** COMPETES_WITH, SUBSTITUTES, CITES
- **cast rule (13):** Cast every battlecard cell to CLAIM + LANG_CLAIM_ROW. Guessed cells are F.
- **language tags:** LANG_PROOF, LANG_EVIDENCE_STRENGTH, LANG_COMPETE_PROGRAM, LANG_PROBLEM_POINT, LANG_VENDOR, LANG_CLAIM_ROW, LANG_COMPARISON_MATRIX, LANG_MISSING_PROOF, LANG_LIST_PRICE_ABSENT, LANG_PROOF_NEEDED_QUADRANT, LANG_POSITIONING_LANE, LANG_CAPABILITY_ATOM…

Claim-row, missing-proof, list-price-absent, comparison-matrix. Battlecard cells become tags.

### 12. Funding Momentum

- **binary:** `BIN.REF.FUNDING_MOMENTUM`
- **use:** `work --binary BIN.REF.FUNDING_MOMENTUM` on a dump callback or tagged graph
- **intent / family / group:** capital / timeline / INVESTOR
- **kinds:** Company, Investor, Fund, MarketSignal, Event
- **rels:** INVESTS_IN, RAISES_FROM, SIGNALS
- **cast rule (13):** Cast funding signals to momentum direction + vintage. Amount stays a PROP, direction is this tag.
- **language tags:** LANG_FUNDING_HEAT, LANG_RECENCY_HEAT, LANG_CO_INVESTOR, LANG_CAPITAL_PATH, LANG_LEAD_INVESTOR, LANG_FOLLOW_ON, LANG_GROWING, LANG_CONSOLIDATING, LANG_ROUND_SIZE, LANG_INVESTOR_QUALITY, LANG_MOMENTUM_UP, LANG_MOMENTUM_FLAT…

Round size, investor quality, momentum up/flat/down, vintage, recency-heat.

### 13. Influence Network

- **binary:** `BIN.REF.INFLUENCE_NETWORK`
- **use:** `work --binary BIN.REF.INFLUENCE_NETWORK` on a dump callback or tagged graph
- **intent / family / group:** opportunity / network / NETWORK
- **kinds:** Person, Company, Investor, Fund
- **rels:** INFLUENCES, WORKS_AT, FOUNDED
- **cast rule (13):** Cast creator/operator/analyst before walking INFLUENCES.
- **language tags:** LANG_ATTENTION_HEAT, LANG_ECOSYSTEM_CHAMPION, LANG_INFLUENCE_PATH, LANG_CREATOR_NODE, LANG_OPERATOR_NODE, LANG_ANALYST_NODE, LANG_TALENT_LINEAGE, LANG_PREVIOUS_COMPANY, LANG_OPERATOR_PATH, LANG_NAMING_ACT

Creator / operator / analyst / influencer / journalist plus influence-path.

### 14. Category Stack

- **binary:** `BIN.REF.CATEGORY_STACK`
- **use:** `work --binary BIN.REF.CATEGORY_STACK` on a dump callback or tagged graph
- **intent / family / group:** understand / swimlane / CANVAS
- **kinds:** Company, Product, Market, Category
- **rels:** IN_CATEGORY, OFFERED_BY
- **cast rule (13):** Cast stack layer. Middleware is allowed without claiming the company is only middleware.
- **language tags:** LANG_CATEGORY_NEIGHBORHOOD, LANG_CORE_CATEGORY_MEMBER, LANG_LAYER_INFRA, LANG_LAYER_MID, LANG_LAYER_APP, LANG_LAYER_DIST, LANG_STACK_INFRA, LANG_STACK_MIDDLEWARE, LANG_STACK_APPLICATION, LANG_STACK_DISTRIBUTION, LANG_PLATFORM_PULL

Four stack lanes + middleware positioning language + distribution owner.

### 15. Market Map Canvas

- **binary:** `BIN.REF.MARKET_MAP_CANVAS`
- **use:** `work --binary BIN.REF.MARKET_MAP_CANVAS` on a dump callback or tagged graph
- **intent / family / group:** understand / quadrant / CANVAS
- **kinds:** Company, Market, Category, Portfolio, MarketMap, Workspace
- **rels:** IN_CATEGORY, PLACED_IN
- **cast rule (13):** Cast active-context + field-contract. Canvas is not a new map type.
- **language tags:** LANG_CATEGORY_NEIGHBORHOOD, LANG_ONE_SCREEN, LANG_ACTIVE_CONTEXT, LANG_FLEXIBLE_PLACEMENT, LANG_EXECUTIVE_BRIEF, LANG_ACTIVE_MARKET_CONTEXT, LANG_FIELD_CONTRACT

One-screen, active context, field-contract, agent-callable. Canvas talks structure not coverage.

### 16. ICP Expansion

- **binary:** `BIN.REF.ICP_EXPANSION`
- **use:** `work --binary BIN.REF.ICP_EXPANSION` on a dump callback or tagged graph
- **intent / family / group:** opportunity / quadrant / LANDSCAPE
- **kinds:** Account, Person, Customer
- **rels:** HAS_PERSONA, ADJACENT_TO, BUYS
- **cast rule (13):** Cast proven-customer-context first. Adjacent buyer without that context is limitation.
- **language tags:** LANG_CATEGORY_DISTANCE, LANG_BUYER_ROLE, LANG_PERSONA_LINK, LANG_EXPANSION_STEP, LANG_ADJACENT_BUYER, LANG_EXPANSION_PATH, LANG_PROVEN_CUSTOMER_CONTEXT, LANG_UNDERSERVED_CLUSTER, LANG_OPPORTUNITY_GAP, LANG_FIT, LANG_WARM_PATH_ACCOUNT

Adjacent buyer, proven customer context, expansion path, ICP archetypes.

### 17. Market White Space

- **binary:** `BIN.REF.MARKET_WHITE_SPACE`
- **use:** `work --binary BIN.REF.MARKET_WHITE_SPACE` on a dump callback or tagged graph
- **intent / family / group:** opportunity / quadrant / LANDSCAPE
- **kinds:** Company, Market, Category, MarketSignal
- **rels:** IN_CATEGORY, ADJACENT_TO
- **cast rule (13):** Cast density language only when the module calculator is on the plan. Tags name the field; they do not invent the number.
- **language tags:** LANG_CROWDED_AREA, LANG_OPEN_AREA, LANG_CATEGORY_DISTANCE, LANG_MISSING_PROOF, LANG_UNDERSERVED_CLUSTER, LANG_OPPORTUNITY_GAP, LANG_NEED_INTENSITY, LANG_SUPPLY_DENSITY, LANG_WHITE_SPACE_DENSITY, LANG_PROOF_NEEDED_QUADRANT, LANG_CATEGORY_CREATOR_LANE, LANG_GAP…

Need / supply / white-space density, underserved cluster, proof-needed quadrant, bridge-band.

### 18. Account Targeting

- **binary:** `BIN.REF.ACCOUNT_TARGETING`
- **use:** `work --binary BIN.REF.ACCOUNT_TARGETING` on a dump callback or tagged graph
- **intent / family / group:** opportunity / swimlane / CANVAS
- **kinds:** Account, Company, Person
- **rels:** TARGETED_BY, PATH_TO, HAS_PERSONA
- **cast rule (13):** Cast fit/timing/warm-path independently. Do not blend into one priority scalar here.
- **language tags:** LANG_PERSONA_LINK, LANG_VENDOR, LANG_STRATEGIC_TARGET, LANG_WARM_PATH_HOP, LANG_ACTIVE_CONTEXT, LANG_ADJACENT_BUYER, LANG_PROVEN_CUSTOMER_CONTEXT, LANG_FIT, LANG_TIMING, LANG_WARM_PATH_ACCOUNT, LANG_RANKED_ACCOUNT, LANG_ACTIVE_MARKET_CONTEXT

Fit, timing, ranked account, warm-path account, RevOps / account-intel industry tag.

### 19. Narrative Positioning

- **binary:** `BIN.REF.NARRATIVE_POSITIONING`
- **use:** `work --binary BIN.REF.NARRATIVE_POSITIONING` on a dump callback or tagged graph
- **intent / family / group:** compare / quadrant / LANDSCAPE
- **kinds:** Company, Claim, Source, Content
- **rels:** ASSERTS, CITES
- **cast rule (13):** Cast framing + lane. Structural-claim without LANG_PROOF stays visible as jargon-risk.
- **language tags:** LANG_DIFFERENTIATION, LANG_ATTENTION_HEAT, LANG_PLATFORM_THESIS, LANG_CLAIM_ROW, LANG_CREATOR_NODE, LANG_FRAMING, LANG_POSITIONING_LANE, LANG_DISTINCTIVENESS, LANG_CATEGORY_CREATOR_LANE, LANG_STRUCTURAL_CLAIM, LANG_NAMING_ACT, LANG_CATEGORY_FORMATION…

Framing, positioning lane, distinctiveness, category-creator lane, structural-claim warning.

### 20. Founder/Operator Lineage

- **binary:** `BIN.REF.FOUNDER_OPERATOR_LINEAGE`
- **use:** `work --binary BIN.REF.FOUNDER_OPERATOR_LINEAGE` on a dump callback or tagged graph
- **intent / family / group:** opportunity / network / NETWORK
- **kinds:** Person, Company
- **rels:** FOUNDED, WORKS_AT, PREVIOUSLY_AT
- **cast rule (13):** Cast founder/operator/spinout. PREVIOUSLY_AT residual + PREVIOUS_COMPANY tag may both fire.
- **language tags:** LANG_HIRING_HEAT, LANG_INFLUENCE_PATH, LANG_OPERATOR_NODE, LANG_TALENT_LINEAGE, LANG_SPIN_OUT, LANG_PREVIOUS_COMPANY, LANG_OPERATOR_PATH

Founder / operator / spinout / previous-company / talent-lineage / accelerator.

### 21. Product Capability

- **binary:** `BIN.REF.PRODUCT_CAPABILITY`
- **use:** `work --binary BIN.REF.PRODUCT_CAPABILITY` on a dump callback or tagged graph
- **intent / family / group:** compare / swimlane / CANVAS
- **kinds:** Product, Company, Claim
- **rels:** HAS_CAPABILITY, OFFERED_BY, SUBSTITUTES
- **cast rule (13):** Cast capability atom + proof-backed vs missing-proof. Proof is CLAIM, not a product prop.
- **language tags:** LANG_PROOF, LANG_EVIDENCE_STRENGTH, LANG_PROBLEM_POINT, LANG_PROOF_POINT, LANG_INTEGRATION, LANG_INITIAL_WEDGE, LANG_LAYER_INFRA, LANG_LAYER_MID, LANG_LAYER_APP, LANG_CLAIM_ROW, LANG_COMPARISON_MATRIX, LANG_MISSING_PROOF…

Capability atom, gap, proof-backed strength, missing-proof lane, point-solution vs suite.

### 22. Market Entry

- **binary:** `BIN.REF.MARKET_ENTRY`
- **use:** `work --binary BIN.REF.MARKET_ENTRY` on a dump callback or tagged graph
- **intent / family / group:** opportunity / brief / BRIEF
- **kinds:** Market, Category, Company
- **rels:** OPERATES_IN, PATH_TO, PARTNERS_WITH
- **cast rule (13):** Cast wedge + risk + regulator. Attractiveness is module language, tag only names it.
- **language tags:** LANG_CAPITAL_PATH, LANG_STRATEGIC_TARGET, LANG_WARM_PATH_HOP, LANG_PARTNER_PATH, LANG_INITIAL_WEDGE, LANG_OPPORTUNITY_GAP, LANG_ENTRY_WEDGE, LANG_PARTNER_PATH_ENTRY, LANG_MARKET_RISK, LANG_REGIONAL_RISK, LANG_ATTRACTIVENESS, LANG_EXECUTIVE_BRIEF

Entry wedge, partner path, market/regional risk, attractiveness, regulator / compliance seats.

### 23. Ecosystem Gravity

- **binary:** `BIN.REF.ECOSYSTEM_GRAVITY`
- **use:** `work --binary BIN.REF.ECOSYSTEM_GRAVITY` on a dump callback or tagged graph
- **intent / family / group:** opportunity / hub / NETWORK
- **kinds:** Company, Product
- **rels:** PARTNERS_WITH, INTEGRATES_WITH
- **cast rule (13):** Cast hub/spoke/pull. Focal is one node, not a second gravity score.
- **language tags:** LANG_CLUSTER_GRAVITY, LANG_BUILDER_NODE, LANG_PROJECT_NODE, LANG_INTEGRATION, LANG_ECOSYSTEM_CHAMPION, LANG_PARTNER_PATH, LANG_PLATFORM_THESIS, LANG_LAYER_DIST, LANG_STACK_DISTRIBUTION, LANG_PARTNER_PATH_ENTRY, LANG_PULL, LANG_GRAVITY_HUB…

Pull, gravity hub, platform owner, spoke, distribution owner, focal.

### 24. Category Creation Timeline

- **binary:** `BIN.REF.CATEGORY_CREATION_TIMELINE`
- **use:** `work --binary BIN.REF.CATEGORY_CREATION_TIMELINE` on a dump callback or tagged graph
- **intent / family / group:** output / timeline / BRIEF
- **kinds:** Event, MarketSignal, Company, Content
- **rels:** ANNOUNCED, OCCURRED_AT, SIGNALS
- **cast rule (13):** Cast naming-act vs funding-act. Do not collapse language events into funding events.
- **language tags:** LANG_LAUNCH_HEAT, LANG_EMERGING, LANG_LIFECYCLE_CURVE, LANG_CREATOR_NODE, LANG_FRAMING, LANG_CATEGORY_CREATOR_LANE, LANG_SPIN_OUT, LANG_NAMING_ACT, LANG_FUNDING_ACT, LANG_CATEGORY_FORMATION, LANG_TEMPORAL_SIGNAL, LANG_COMPACT_NARRATIVE…

Naming act vs funding act, category formation, temporal signal, creator node.

### 25. Market Intelligence Brief

- **binary:** `BIN.REF.MARKET_INTELLIGENCE_BRIEF`
- **use:** `work --binary BIN.REF.MARKET_INTELLIGENCE_BRIEF` on a dump callback or tagged graph
- **intent / family / group:** output / brief / BRIEF
- **kinds:** Company, Market, Category, Claim, Source, Portfolio, MarketMap, Workspace
- **rels:** ASSERTS, CITES, CONTAINS_MAP
- **cast rule (13):** Cast brief + completeness + field-contract.
- **language tags:** LANG_COMPETE_PROGRAM, LANG_LIST_PRICE_ABSENT, LANG_ANALYST_NODE, LANG_ONE_SCREEN, LANG_ACTIVE_CONTEXT, LANG_FRAMING, LANG_STRUCTURAL_CLAIM, LANG_EXECUTIVE_BRIEF, LANG_ACTIVE_MARKET_CONTEXT, LANG_COMPACT_NARRATIVE, LANG_COMPLETENESS_METRIC, LANG_FIELD_CONTRACT…

Executive brief, completeness metric, field contract, refreshable, active market context.

