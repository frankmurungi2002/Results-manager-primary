/**
 * The right-hand side of the sign-in screen: four pictures that crossfade
 * every few seconds, each with a caption card and previous/next buttons.
 * A school's own photos (Settings → School → Sign-in pictures) replace the
 * built-in pictures slot by slot.
 */

import { useEffect, useState } from "react";
import {
  CalendarCheck,
  ChevronLeft,
  ChevronRight,
  ClipboardCheck,
  FileText,
  GraduationCap,
  MessageSquareText,
  type LucideIcon,
} from "lucide-react";

import { api } from "../lib/api";
import { BUILT_IN_SCENES } from "./LoginScenes";
import { cx } from "./ui";

const SLIDE_MS = 6000;

const CAPTIONS: { title: string; text: string; meta: [LucideIcon, string][] }[] = [
  {
    title: "Every learner, every mark",
    text: "Enter marks in a grid, a form or from Excel. Grades, aggregates, divisions and positions work themselves out.",
    meta: [
      [ClipboardCheck, "Nursery to P7"],
      [FileText, "UNEB grading"],
    ],
  },
  {
    title: "Registers and timetables",
    text: "The daily register texts the parents of absent learners, and timetables never book a teacher in two places.",
    meta: [
      [CalendarCheck, "Daily register"],
      [MessageSquareText, "SMS to parents"],
    ],
  },
  {
    title: "From Baby Class to PLE",
    text: "Follow every learner term after term, with last term's results printed beside this term's on the report card.",
    meta: [
      [GraduationCap, "Progress each term"],
      [ClipboardCheck, "Promotion ready"],
    ],
  },
  {
    title: "Report cards in minutes",
    text: "Nursery and primary report cards with your school's name and logo, printed for a whole class in one go.",
    meta: [
      [FileText, "One page per learner"],
      [ClipboardCheck, "Comments and positions"],
    ],
  },
];

export function LoginShowcase() {
  const [photos, setPhotos] = useState<(string | null)[]>([null, null, null, null]);
  const [index, setIndex] = useState(0);
  const [paused, setPaused] = useState(false);

  useEffect(() => {
    api
      .getLoginImages()
      .then(setPhotos)
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (paused) return;
    const timer = window.setTimeout(() => setIndex((current) => (current + 1) % CAPTIONS.length), SLIDE_MS);
    return () => window.clearTimeout(timer);
  }, [index, paused]);

  const go = (step: number) => setIndex((current) => (current + step + CAPTIONS.length) % CAPTIONS.length);
  const caption = CAPTIONS[index]!;

  return (
    <aside
      className="login-showcase"
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      aria-label="About Phantom School Manager"
    >
      {CAPTIONS.map((_, slide) => {
        const Scene = BUILT_IN_SCENES[slide]!;
        const photo = photos[slide];
        return (
          <div key={slide} className={cx("showcase-slide", slide === index && "is-active")} aria-hidden={slide !== index}>
            {photo ? <img className="showcase-photo" src={photo} alt="" /> : <Scene />}
          </div>
        );
      })}

      <div className="showcase-caption" key={index}>
        <div className="showcase-title">{caption.title}</div>
        <p className="showcase-text">{caption.text}</p>
        <div className="showcase-meta">
          {caption.meta.map(([Icon, label]) => (
            <span key={label}>
              <Icon size={12} />
              {label}
            </span>
          ))}
        </div>
      </div>

      <div className="showcase-controls">
        <button className="showcase-arrow" onClick={() => go(-1)} aria-label="Previous picture">
          <ChevronLeft size={16} />
        </button>
        <button className="showcase-arrow" onClick={() => go(1)} aria-label="Next picture">
          <ChevronRight size={16} />
        </button>
      </div>

      <div className="showcase-dots" role="tablist">
        {CAPTIONS.map((entry, slide) => (
          <button
            key={entry.title}
            role="tab"
            aria-selected={slide === index}
            aria-label={entry.title}
            className={cx("showcase-dot", slide === index && "is-active")}
            onClick={() => setIndex(slide)}
          />
        ))}
      </div>
    </aside>
  );
}
