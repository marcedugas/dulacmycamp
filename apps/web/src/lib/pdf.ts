import type { jsPDF } from 'jspdf';
import {
  endOfMonth,
  endOfWeek,
  format,
  isSameMonth,
  startOfMonth,
  startOfWeek,
} from 'date-fns';
import type { CalendarView, DayCell } from '../components/CampCalendar';
import { daysInclusive, toKey } from './dates';

/**
 * Renders the calendar straight to PDF with jsPDF drawing primitives.
 *
 * Deliberately not a screenshot of the live DOM: Tailwind v4 emits `oklch()`
 * colours, which html2canvas cannot parse, and a vector grid is sharper and
 * a fraction of the file size anyway. Always light-themed for printing.
 */

type RGB = [number, number, number];

const INK: RGB = [26, 26, 26];
const MUTED: RGB = [107, 98, 85];
const SAND: RGB = [221, 210, 194];
const CREAM: RGB = [250, 247, 242];
const FOREST: RGB = [45, 90, 39];
const AMBER: RGB = [253, 230, 138];
const AMBER_INK: RGB = [120, 53, 15];
const CLAY: RGB = [155, 50, 38];

const WEEKDAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

const PAGE_W = 297;
const PAGE_H = 210;
const MARGIN = 12;

function drawMonth(
  doc: jsPDF,
  month: Date,
  index: Map<string, DayCell>,
  capacityLimit: number,
) {
  const gridTop = 34;
  const gridBottom = PAGE_H - 26;
  const gridW = PAGE_W - MARGIN * 2;
  const colW = gridW / 7;

  const days = daysInclusive(startOfWeek(startOfMonth(month)), endOfWeek(endOfMonth(month)));
  const rows = days.length / 7;
  const rowH = (gridBottom - gridTop) / rows;

  // ── header ──
  doc.setTextColor(...INK);
  doc.setFont('helvetica', 'bold');
  doc.setFontSize(20);
  doc.text('Dulac My Camp', MARGIN, 20);

  doc.setFontSize(13);
  doc.setTextColor(...FOREST);
  doc.text(format(month, 'MMMM yyyy'), MARGIN, 28);

  doc.setFont('helvetica', 'normal');
  doc.setFontSize(8);
  doc.setTextColor(...MUTED);
  doc.text(`Generated ${format(new Date(), 'MMM d, yyyy')}`, PAGE_W - MARGIN, 20, {
    align: 'right',
  });

  // ── weekday header ──
  doc.setFont('helvetica', 'bold');
  doc.setFontSize(8);
  doc.setTextColor(...MUTED);
  WEEKDAYS.forEach((d, i) => {
    doc.text(d.toUpperCase(), MARGIN + i * colW + 2, gridTop - 2);
  });

  // ── cells ──
  days.forEach((date, i) => {
    const col = i % 7;
    const row = Math.floor(i / 7);
    const x = MARGIN + col * colW;
    const y = gridTop + row * rowH;

    const cell = index.get(toKey(date));
    const outside = !isSameMonth(date, month);

    // Ground.
    const booked = (cell?.approved.length ?? 0) > 0;
    const pending = (cell?.pending.length ?? 0) > 0;
    const blackout = Boolean(cell?.blackout);

    doc.setFillColor(...(blackout ? SAND : booked ? FOREST : pending ? AMBER : CREAM));
    doc.setDrawColor(...SAND);
    doc.setLineWidth(0.2);
    doc.rect(x, y, colW, rowH, 'FD');

    // Date number.
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(9);
    if (outside) doc.setTextColor(190, 182, 170);
    else if (booked) doc.setTextColor(...CREAM);
    else if (pending) doc.setTextColor(...AMBER_INK);
    else doc.setTextColor(...INK);
    doc.text(format(date, 'd'), x + 2, y + 5);

    if (outside || !cell) return;

    // Status line.
    doc.setFont('helvetica', 'normal');
    doc.setFontSize(7);
    const label = blackout
      ? 'Blackout'
      : booked
        ? `Booked${cell.approved.length > 1 ? ` x${cell.approved.length}` : ''}`
        : pending
          ? `Pending${cell.pending.length > 1 ? ` x${cell.pending.length}` : ''}`
          : '';
    if (label) doc.text(label, x + 2, y + 10);

    // Adults on approved stays, and a flag when the camp is over capacity.
    if (booked) {
      doc.setTextColor(...CREAM);
      doc.text(`${cell.adults} adults`, x + 2, y + 14);
      if (cell.adults > capacityLimit) {
        doc.setTextColor(255, 214, 210);
        doc.text('! over capacity', x + 2, y + 18);
      }
    }

    // Events sit bottom-left so they never collide with the status line.
    if (cell.events.length > 0) {
      doc.setTextColor(...(booked ? CREAM : MUTED));
      doc.setFontSize(6.5);
      const names = cell.events.map((e) => e.name).join(', ');
      doc.text(doc.splitTextToSize(names, colW - 4)[0] ?? '', x + 2, y + rowH - 2);
    }
  });

  // ── legend ──
  const legend: { label: string; fill: RGB; ink?: RGB }[] = [
    { label: 'Available', fill: CREAM },
    { label: 'Booked', fill: FOREST },
    { label: 'Pending', fill: AMBER },
    { label: 'Blackout', fill: SAND },
  ];

  let lx = MARGIN;
  const ly = PAGE_H - 18;
  doc.setFontSize(8);
  doc.setFont('helvetica', 'normal');
  for (const item of legend) {
    doc.setFillColor(...item.fill);
    doc.setDrawColor(...SAND);
    doc.rect(lx, ly, 5, 4, 'FD');
    doc.setTextColor(...INK);
    doc.text(item.label, lx + 7, ly + 3.2);
    lx += 7 + doc.getTextWidth(item.label) + 8;
  }
  doc.setTextColor(...CLAY);
  doc.text(`!  Over capacity (sleeps ${capacityLimit} adults)`, lx, ly + 3.2);

  doc.setTextColor(...MUTED);
  doc.setFontSize(7.5);
  doc.text('(c) 2026 Dulac My Camp', MARGIN, PAGE_H - 8);
}

/** jsPDF is ~380 kB with its optional deps, so it is fetched on first export. */
export async function exportCalendarPdf(opts: {
  view: CalendarView;
  anchor: Date;
  index: Map<string, DayCell>;
  capacityLimit: number;
}): Promise<void> {
  const { view, anchor, index, capacityLimit } = opts;
  const { jsPDF: JsPDF } = await import('jspdf');
  const doc = new JsPDF({ orientation: 'landscape', unit: 'mm', format: 'a4' });

  // The year view prints as twelve pages; every other view prints the month
  // it is looking at, since a week grid alone wastes a landscape sheet.
  const months =
    view === 'year'
      ? Array.from({ length: 12 }, (_, m) => new Date(anchor.getFullYear(), m, 1))
      : [startOfMonth(anchor)];

  months.forEach((month, i) => {
    if (i > 0) doc.addPage();
    drawMonth(doc, month, index, capacityLimit);
  });

  const suffix = view === 'year' ? format(anchor, 'yyyy') : format(anchor, 'yyyy-MM');
  doc.save(`dulac-my-camp-calendar-${suffix}.pdf`);
}
