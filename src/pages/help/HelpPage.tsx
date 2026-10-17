/**
 * Help Center — searchable topic cards, FAQs, and support entry
 * points. Deliberately AI-free (owner direction: no assistant robots,
 * no AI pop-ups) — human-authored guides and support channels only.
 */
import { useMemo, useState } from "react";
import { motion } from "framer-motion";
import {
  ChatIcon,
  ChevronDownIcon,
  HardDriveIcon,
  HealthIcon,
  KeyIcon,
  LifeBuoyIcon,
  SearchIcon,
  SupportIcon,
  Trash2Icon,
  WrenchIcon,
} from "../../shell/icons";

interface Topic {
  icon: typeof HardDriveIcon;
  title: string;
  body: string;
  tag: string;
}

const TOPICS: Topic[] = [
  {
    icon: Trash2Icon,
    title: "Recover deleted files",
    body: "Pick the drive where files were deleted, scan, then filter by type or date. Recycle Bin and Desktop have one-click entries on the Recover Data page.",
    tag: "Recovery",
  },
  {
    icon: HardDriveIcon,
    title: "Lost partitions & whole disks",
    body: "Scanning a full disk surfaces lost partitions automatically. Eligible partitions get a direct repair path — restore access without complex procedures.",
    tag: "Recovery",
  },
  {
    icon: WrenchIcon,
    title: "Repair corrupted files",
    body: "Enable file repair during recovery and corrupted photos, videos and documents are rebuilt automatically. You can preview repaired files before saving.",
    tag: "Repair",
  },
  {
    icon: HealthIcon,
    title: "Read disk health",
    body: "Disk Tools shows live S.M.A.R.T. attributes, temperatures and risk verdicts for every disk — free forever, no license needed.",
    tag: "Health",
  },
  {
    icon: KeyIcon,
    title: "Activate your license",
    body: "Enter your DW-XXXXX-XXXXX-XXXXX-XXXXX code from the Activate dialog. No reinstallation or rescan is required after activation.",
    tag: "Licensing",
  },
  {
    icon: SupportIcon,
    title: "SD Card Rescue",
    body: "For cards that are inaccessible, not recognized, or prompt to format — DiskWizard first restores access to the card, then continues with file recovery.",
    tag: "Rescue",
  },
];

const FAQS: { q: string; a: string }[] = [
  {
    q: "Where should I save recovered files?",
    a: "Always to a different drive than the one you lost data from — saving to the original drive risks permanently overwriting the very files you are recovering. The destination picker warns you and blocks the source partition.",
  },
  {
    q: "Quick scan or deep scan?",
    a: "DiskWizard runs a quick metadata pass first (most deletions surface there), then continues into a deep sector-by-sector scan automatically — carving files by signature even when paths are gone. You can preview and recover from the quick results while the deep pass runs.",
  },
  {
    q: "Why do some files say 'Path lost'?",
    a: "Deep scanning carves files straight from raw sectors. When a file's folder chain can't be reconstructed, it lands under File Path Lost — often still perfectly recoverable, just without its original location.",
  },
  {
    q: "What does the condition chip mean?",
    a: "Every found file is graded before recovery: Excellent (fully readable), Good (mostly readable) or Poor (partially overwritten). Use it to decide what to recover first.",
  },
  {
    q: "What is 'Essential results'?",
    a: "DiskWizard hides duplicates and nonessential files by default — roughly 65% less to review in our testing. Switch to Complete results at any moment to see everything.",
  },
  {
    q: "Is one license enough for my PCs?",
    a: "One license activates one Windows device and one macOS device. Deactivate on the old device (Settings → License) to move it to new hardware.",
  },
];

export function HelpPage() {
  const [query, setQuery] = useState("");
  const [openFaq, setOpenFaq] = useState<number | null>(0);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return TOPICS;
    return TOPICS.filter((t) => `${t.title} ${t.body} ${t.tag}`.toLowerCase().includes(q));
  }, [query]);

  const filteredFaqs = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return FAQS;
    return FAQS.filter((f) => `${f.q} ${f.a}`.toLowerCase().includes(q));
  }, [query]);

  return (
    <div className="dw-page dw-page-help dw-scroll">
      <header className="dw-page-head">
        <div>
          <div className="dw-eyebrow">Help Center</div>
          <h1 className="dw-page-title dw-display">How can we help?</h1>
          <p className="dw-page-sub">Guides, answers and human support — no bots, no runarounds.</p>
        </div>
        <div className="dw-help-search">
          <SearchIcon size={15} className="dw-search-icon" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search help topics…"
            aria-label="Search help"
          />
        </div>
      </header>

      <section className="dw-section">
        <div className="dw-help-grid">
          {filtered.map((t, i) => (
            <motion.article
              key={t.title}
              className="dw-card dw-help-card"
              initial={{ opacity: 0, y: 14 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: i * 0.045, duration: 0.3, ease: [0.2, 0.8, 0.3, 1] }}
            >
              <span className="dw-help-icon">
                <t.icon size={19} strokeWidth={1.7} />
              </span>
              <div className="dw-help-copy">
                <div className="dw-help-title">{t.title}</div>
                <p className="dw-help-body">{t.body}</p>
                <span className="dw-tag is-accent">{t.tag}</span>
              </div>
            </motion.article>
          ))}
          {filtered.length === 0 && (
            <div className="dw-help-noresults">No topics match “{query}” — try a different wording, or contact support below.</div>
          )}
        </div>
      </section>

      <section className="dw-section">
        <div className="dw-section-head">
          <h2 className="dw-section-title">Frequently asked questions</h2>
        </div>
        <div className="dw-faq-list">
          {filteredFaqs.map((f, i) => {
            const open = openFaq === i;
            return (
              <div key={f.q} className={`dw-faq ${open ? "is-open" : ""}`}>
                <button type="button" className="dw-faq-q" onClick={() => setOpenFaq(open ? null : i)} aria-expanded={open}>
                  {f.q}
                  <ChevronDownIcon size={16} className="dw-faq-chev" />
                </button>
                {open && <p className="dw-faq-a">{f.a}</p>}
              </div>
            );
          })}
        </div>
      </section>

      <section className="dw-section">
        <div className="dw-help-support">
          <div className="dw-card dw-help-support-card">
            <span className="dw-help-icon">
              <ChatIcon size={19} strokeWidth={1.7} />
            </span>
            <div className="dw-help-copy">
              <div className="dw-help-title">Contact support</div>
              <p className="dw-help-body">Real humans, 24/7 — we answer every ticket.</p>
            </div>
            <a className="dw-outline-button" href="https://diskwizard.app/support" target="_blank" rel="noreferrer">
              Open a ticket
            </a>
          </div>
          <div className="dw-card dw-help-support-card">
            <span className="dw-help-icon">
              <LifeBuoyIcon size={19} strokeWidth={1.7} />
            </span>
            <div className="dw-help-copy">
              <div className="dw-help-title">Data recovery service</div>
              <p className="dw-help-body">Physically failed drive? Our lab engineers can help.</p>
            </div>
            <a className="dw-outline-button" href="https://diskwizard.app/service" target="_blank" rel="noreferrer">
              Learn more
            </a>
          </div>
        </div>
      </section>
    </div>
  );
}
