# Book presentation

Desktop, Android and iOS use a paper surface for book navigation. This transition is separate from chapter pagination and page-curl gestures; reader text and illustrations are never scaled.

## Sequence

- At a visible shelf cover, dissolve the actual cover into the current reader paper color (100 ms).
- Parse the EPUB and prepare the first native reading viewport while the paper expands. Font registration, publication styles, annotations and visible illustrations belong to preparation, not a second loading stage after expansion.
- Expand most of the distance first. If preparation is still pending, keep advancing below the full-screen endpoint; release the final 80 ms stretch only after the first viewport is ready. Normal geometry takes about 380 ms, without overshoot; slow books can extend this phase.
- At the endpoint, reveal already prepared content by fading the paper away (160 ms). A delayed progress indicator remains available during genuinely slow preparation (600 ms on mobile, 800 ms on desktop).
- On close, cover the reader in its current paper color (140 ms), restore the shelf below it, contract to the cover (300 ms), and dissolve the paper back into the actual cover (100 ms).

A missing or offscreen source uses an opacity transition. The placeholder for a book without cover art is also a valid source. Changing the reading theme changes the closing paper color. Mobile shelves stay mounted so their search and scroll state survive; desktop retains the native shelf paint list during expansion. The closing destination is the opened book's cover in the retained shelf order; recent-reading promotion waits until closing finishes.

## Host integration

- Desktop: `book_presentation.rs` owns the transition. Parsing remains on the existing background worker; results mount immediately under the paper. Visible illustration textures gate reveal, while offscreen prefetch does not. Books with unchanged embedded fonts retain the registered system font atlas. The foreground paper/input layer stays above reader chrome. Escape cancels the pending result. Resizing invalidates the source geometry and uses the fade fallback.
- Android: `BookPresentation` mounts `ReaderScreen` during expansion. The resume chapter is loaded on the IO worker; publication styling stays lazy rather than walking every chapter. Pagination runs on a background worker with a bounded cache validated against geometry and chapter styles. Readiness requires initial layout/styles, viewport image preparation and frame boundaries. Scroll mode tracks only intersecting image bounds; broken images finish with their fallback. Effect keys use reference identity so comparisons cannot traverse lazy chapters. Toolbar and system back handlers close through the same host. Cancelled parse jobs use a generation guard so their finalizers cannot overwrite a later loading state. System-bar contrast switches when paper reaches the viewport edge. System disabled animations use the opacity fallback.
- iOS: a same-window SwiftUI host replaces the system full-screen modal. The current chapter styles and annotation load attempt precede initial native layout; visible attachment images must finish decoding before the preparation callback. UIKit owns paper geometry/opacity animations so Core Animation can continue during native text preparation. Reader controls use the actual window safe area while only the paper background extends to the edges; the host controls close and sync navigation. Opening tokens discard stale asynchronous results. Reduce Motion uses opacity rather than bounds interpolation.

The paper is a plain color, not a scaled screenshot of the reader or a stretched cover texture. Progress saving, reading layout modes, annotations and image caches retain their existing paths.

## Readiness and visibility

Readiness is not an arbitrary minimum loading timer. Failed optional annotations or broken images use their existing fallback so they cannot hold the transition indefinitely. A malformed publication can still take longer to parse or lay out; the remaining expansion covers that preparation rather than displaying a full-screen post-animation spinner.

Preparing an early-mounted reader is separate from displaying it. The native
controller now stays at zero alpha during expansion, with the workspace/chrome
hidden as well. Its layout and visible-image preparation still run concurrently.
The paper's actual UIKit animation completion releases visibility only when the
paper has reached the viewport. A second completion removes the paper after its
fade; no assumed 80/160 ms sleep controls these opening boundaries. Unchanged
SwiftUI updates do not restart the native paper animation. Transition tokens
reject callbacks from cancelled books.

## Deferred shelf promotion

Reading metadata still updates immediately. Desktop, Android and iOS retain the
shelf's book-ID order during opening, reading and closing, so the paper returns
to the cover that was actually opened. Only after the closing paper has shrunk
and dissolved does the shelf apply the latest recent-reading order. The whole
card (cover, title, progress and actions) then moves to its new slot over 380 ms;
the other cards move into the vacated slots. This also applies across rows.
The promoted card draws above its neighbours while moving. Alphabetical order
stays alphabetical. Reduced-motion settings skip the shelf travel.

Desktop uses fixed destination slots with independently positioned native card
UIs, so hit boxes follow the cards. Android uses keyed lazy-grid item placement.
iOS uses stable IDs and matched geometry in the existing native shelf. Progress,
cover lookup and selection remain tied to book identity rather than its index.
