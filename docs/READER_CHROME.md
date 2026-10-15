# Reader chrome and paper footers

## iOS

Paged reading places each leaf's page number in a native label owned by
`PaperController`, rather than the SwiftUI controls overlay. Curl, slide and
cover transitions therefore carry distinct outgoing and incoming page numbers.
Each real leaf, including both leaves of a spread, displays its own number;
synthetic blank leaves have no page number. The footer uses 12 pt monospaced
digits, 16 pt trailing spacing and 8 pt spacing above the frozen bottom safe
area. The overflow fallback pins its footer to the scroll viewport.

The optional time and battery information uses the same paper-owned footer,
aligned to the bottom-left. Scroll reading uses viewport footers instead.
The setting is off by default and is available under Themes & Settings > More.
The clock and battery refresh every 15 seconds while the reader is active.
Battery monitoring is reference-counted and restores its prior state when no
reader needs it. An unknown battery level displays time alone. Menu visibility
does not control the setting; toggling it does not invalidate text layout.

The top controls show a centered book title and a second line with chapter
position and title. Paged bottom controls sit above the footer row so the menu
button does not cover the page number.

## Desktop

Top controls and the bottom menu button fade at fixed positions over 180 ms.
Visibility changes do not move the controls or resize the reading viewport;
fading-out controls stop accepting input immediately.
The bottom-right quick menu retains its paint for a 160 ms fade after popup
memory closes. It releases input immediately and reverses the same transition
if reopened before the fade ends.

## Android

The top and bottom controls fade in place over 180 ms and fade out over 140 ms.
Their status/navigation insets remain fixed while system bars change visibility,
so revealing chrome does not slide the controls or reflow the reading paper.
The quick menu expands from its lower trailing corner (92% to full size over
240 ms) with an opacity transition, then contracts over 160 ms on dismissal.
Compose retains the popup through exit and reverses interrupted transitions;
platform animation-duration settings still apply. Dismissing the popup releases
focus immediately, including when opening settings. Back/outside dismissal and
scrolling in small windows use the native popup and scroll container.
Explicit Done/Close buttons in reading settings, fonts, correction, statistics,
reviews and the introduction await the native sheet's downward hide animation
before removing it. Repeated close requests are coalesced; back, scrim taps and
swipes retain Material's existing dismissal path.

Time and battery information remains enabled when controls or the quick menu
are shown. The redundant bottom progress pill is omitted while time/battery is
enabled, preventing it from covering the information. Scroll mode also exposes
time and battery in the bottom-left viewport corner.

## Settings order

Desktop, Android and iOS place Layout Style immediately before Layout, keeping
the Native/Mixed/Classic choice next to the spacing controls it governs.

## Verification

`target/ios-page-footer-20261015/` contains builds, native snapshots and reports.
Eight iOS native cases cover portrait/landscape and curl/slide/cover/none,
checking paper ownership, safe bottom-right placement, correct page numbers
after turns and status toggles without repagination. Native phone screenshots
also show time and page number remaining visible under an open menu.
Android menu persistence and adjacent settings groups were verified through the
actual emulator UI. Desktop settings order was checked in source and compiled;
this change does not add a new desktop runtime acceptance claim.
