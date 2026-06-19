import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { ContainerInfo } from "../types";
import { openLogStream } from "../api";

const MAX_LINES = 5000;

interface Props {
  container: ContainerInfo | null;
}

interface ParsedLine {
  ts: string;
  msg: string;
  level: "err" | "warn" | "info";
  raw: string;
}

function parseLine(raw: string): ParsedLine {
  let ts = "";
  let msg = raw;
  const sp = raw.indexOf(" ");
  if (sp > 0) {
    const head = raw.slice(0, sp);
    if (/^\d{4}-\d{2}-\d{2}T/.test(head)) {
      ts = head.slice(11, 19);
      msg = raw.slice(sp + 1);
    }
  }
  let level: ParsedLine["level"] = "info";
  if (/\b(error|err|fatal|panic|exception)\b/i.test(msg)) level = "err";
  else if (/\b(warn|warning)\b/i.test(msg)) level = "warn";
  return { ts, msg, level, raw };
}

export function LogViewer({ container }: Props) {
  const [lines, setLines] = useState<string[]>([]);
  const [filter, setFilter] = useState("");
  const [paused, setPaused] = useState(false);
  const [wrap, setWrap] = useState(true);

  const bufRef = useRef<string[]>([]);
  const pausedRef = useRef(paused);
  pausedRef.current = paused;
  const scrollRef = useRef<HTMLDivElement>(null);
  const atBottomRef = useRef(true);

  useEffect(() => {
    setLines([]);
    bufRef.current = [];
    if (!container) return;
    const es = openLogStream(container.id, 500, (line) => {
      bufRef.current.push(line);
      if (bufRef.current.length > MAX_LINES) {
        bufRef.current = bufRef.current.slice(bufRef.current.length - MAX_LINES);
      }
    });
    const flush = setInterval(() => {
      if (pausedRef.current || bufRef.current.length === 0) return;
      const incoming = bufRef.current;
      bufRef.current = [];
      setLines((prev) => {
        const next = prev.concat(incoming);
        return next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
      });
    }, 150);
    return () => {
      es.close();
      clearInterval(flush);
    };
  }, [container?.id]);

  const parsed = useMemo(() => {
    const f = filter.trim().toLowerCase();
    const arr = lines.map(parseLine);
    return f ? arr.filter((l) => l.raw.toLowerCase().includes(f)) : arr;
  }, [lines, filter]);

  useLayoutEffect(() => {
    if (atBottomRef.current && scrollRef.current && !paused) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [parsed, paused]);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    atBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  };

  if (!container) {
    return <div className="logs empty-pane">Select a container to view logs</div>;
  }

  return (
    <div className="logs">
      <div className="logbar">
        <input
          className="logfilter"
          placeholder="Filter logs…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <span className="logcount">{parsed.length} lines</span>
        <button className={`btn ${paused ? "on" : ""}`} onClick={() => setPaused((p) => !p)}>
          {paused ? "Resume" : "Pause"}
        </button>
        <button className={`btn ${wrap ? "on" : ""}`} onClick={() => setWrap((w) => !w)}>
          Wrap
        </button>
        <button
          className="btn"
          onClick={() => {
            setLines([]);
            bufRef.current = [];
          }}
        >
          Clear
        </button>
      </div>
      <div
        className={`logbody ${wrap ? "wrap" : "nowrap"}`}
        ref={scrollRef}
        onScroll={onScroll}
      >
        {parsed.length === 0 && (
          <div className="logempty">
            {lines.length === 0
              ? "Waiting for log output… (new lines stream in live)"
              : "No lines match the filter"}
          </div>
        )}
        {parsed.map((l, i) => (
          <div className={`logline ${l.level}`} key={i}>
            {l.ts && <span className="logts">{l.ts}</span>}
            <span className="logmsg">{l.msg}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
