# Recommended Readings & Talks

Primitives, judgment, simplicity, and building things that last.


## Philosophy & Architecture

Simple Made Easy - Rich Hickey (2011)
https://www.infoq.com/presentations/Simple-Made-Easy/
Simple (not braided) is objective. Easy (familiar) is relative. Conflating them is where complexity enters.

Software as a Reflection of Values - Bryan Cantrill
https://corecursive.com/024-software-as-a-reflection-of-values-with-bryan-cantrill/
Technologies reflect the values of their creators. Choose tech by value alignment, not feature checklists.

Choose Boring Technology - Dan McKinley (2015)
https://mcfunley.com/choose-boring-technology
You get limited innovation tokens. Spend them on your actual problem, not on infrastructure.

Semantic Compression / Compression-Oriented Programming - Casey Muratori
https://caseymuratori.com/blog_0015
Make your code usable before you try to make it reusable. Only abstract after seeing the real pattern.

Preventing the Collapse of Civilization - Jonathan Blow (2019)
https://www.youtube.com/watch?v=ZSRHeXYDLko
Software is getting worse. We're losing the ability to build what past generations built.

Boundaries - Gary Bernhardt (2012)
https://www.destroyallsoftware.com/talks/boundaries
Functional core, imperative shell. Separate decisions from dependencies.

The Life of a File - Evan Czaplicki (2017)
https://www.youtube.com/watch?v=XpDsk374LDE
Don't split into modules prematurely. Hold off until you feel the pain.

No Silver Bullet - Fred Brooks (1986)
https://worrydream.com/refs/Brooks_1986_-_No_Silver_Bullet.pdf
Essential complexity (the problem) vs accidental complexity (the tools). Building software will always be hard.

A Philosophy of Software Design - John Ousterhout (book)
https://newsletter.pragmaticengineer.com/p/the-philosophy-of-software-design
Deep modules (simple interface, complex implementation) beat shallow modules. Interview and summary at link.

Simplicity is Complicated - Rob Pike (2015)
https://go.dev/talks/2015/simplicity-is-complicated.slide
A language should have a small number of orthogonal features that combine predictably.


## Manifestos & Long-Form

Redis Manifesto - Salvatore Sanfilippo (antirez)
https://oldblog.antirez.com/post/redis-manifesto.html
"Designing systems is a fight against complexity." Code is like a poem.

Writing System Software: Code Comments - antirez
https://antirez.com/news/124
Why comments are paramount. How to write system software that others can maintain.

The Ruby on Rails Doctrine - DHH
https://rubyonrails.org/doctrine
Convention over configuration. Omakase. The majestic monolith.

Uncurled - Daniel Stenberg (book)
https://un.curl.dev/
30 years of maintaining curl, documented. What it actually takes to sustain infrastructure.

Zero-Cost Abstractions - withoutboats
https://without.boats/blog/zero-cost-abstractions/
Three requirements: no global costs, can't hand-code it better, better UX than the manual alternative.

Inventing the Service Trait - Carl Lerche / Tokio team (2021)
https://tokio.rs/blog/2021-05-14-inventing-the-service-trait
How Tower's Service trait became the foundation of Rust networking.

The Geomys Standard of Care - Filippo Valsorda
https://words.filippo.io/standard-of-care/
Professional independent open-source maintenance as a viable career. A major part of maintaining is saying no.


## Data & Embedded Analytics

Big Data is Dead - Jordan Tigani
https://motherduck.com/videos/the-death-of-big-data-and-why-its-time-to-think-small-jordan-tigani-ceo-motherduck/
Most data is small. The industry over-indexed on scale.

Data Management for Data Science: Towards Embedded Analytics - Raasveldt & Muhleisen (CIDR 2020)
https://hannes.muehleisen.org/publications/CIDR2020-raasveldt-muehleisen-duckdb.pdf
The manifesto for embedded analytical databases. Why "bring the database to the data."

DuckLake: SQL as a Lakehouse Format (2025)
https://ducklake.select/2025/05/27/ducklake-01/
Put lakehouse metadata in a SQL database, not in files. The same architecture as Litehouse.

How Query Engines Work - Andy Grove (book)
https://howqueryengineswork.com/
How to build a query engine from scratch. Informed DataFusion's design.

Lessons Learned Building delta-rs - R. Tyler Croy (2025)
https://www.buoyantdata.com/blog/2025-03-09-lessons-learned-building-delta-rs.html
Five years of building Delta Lake in Rust. Community over code.


## People to Follow

Salvatore Sanfilippo (antirez) - https://antirez.com/
Bryan Cantrill - https://bcantrill.dtrace.org/
Daniel Stenberg - https://daniel.haxx.se/
Andrew Gallant (burntsushi) - https://burntsushi.net/
Armin Ronacher - https://lucumr.pocoo.org/
Simon Willison - https://simonwillison.net/
Filippo Valsorda - https://words.filippo.io/
Mitchell Hashimoto - https://mitchellh.com/writing
matklad (Aleksey Kladov) - https://matklad.github.io/
Andrew Lamb - https://andrew.nerdnetworks.org/
