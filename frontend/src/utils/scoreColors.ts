/**
 * Consistent score color utilities used across the application.
 * This ensures marker colors on the map match score badge colors in cards/popups.
 */

export type ScoreColorLevel = 'green' | 'lime' | 'yellow' | 'orange' | 'red' | 'gray';

/**
 * Get the color level for a given score.
 * Used to determine both marker colors and badge styling.
 */
export function getScoreColorLevel(score: number | null | undefined): ScoreColorLevel {
  if (score === null || score === undefined) return 'gray';
  if (score >= 80) return 'green';
  if (score >= 60) return 'lime';
  if (score >= 40) return 'yellow';
  if (score >= 20) return 'orange';
  return 'red';
}

/**
 * Get the hex color for map markers based on score.
 */
export function getMarkerColor(score: number | null | undefined): string {
  const level = getScoreColorLevel(score);
  switch (level) {
    case 'green': return '#22C55E';
    case 'lime': return '#84CC16';
    case 'yellow': return '#EAB308';
    case 'orange': return '#F97316';
    case 'red': return '#EF4444';
    case 'gray':
    default: return '#6B7280';
  }
}

/**
 * Get Tailwind CSS classes for score badge styling.
 * Returns background and text color classes.
 */
export function getScoreBadgeClasses(score: number | null | undefined): string {
  const level = getScoreColorLevel(score);
  switch (level) {
    case 'green': return 'bg-green-100 text-green-800';
    case 'lime': return 'bg-lime-100 text-lime-800';
    case 'yellow': return 'bg-yellow-100 text-yellow-800';
    case 'orange': return 'bg-orange-100 text-orange-800';
    case 'red': return 'bg-red-100 text-red-800';
    case 'gray':
    default: return 'bg-gray-100 text-gray-800';
  }
}

/**
 * Get Tailwind CSS classes for solid score badge (used in popups).
 * Returns background class with white text.
 */
export function getScoreBadgeSolidClasses(score: number | null | undefined): string {
  const level = getScoreColorLevel(score);
  switch (level) {
    case 'green': return 'bg-green-500';
    case 'lime': return 'bg-lime-500';
    case 'yellow': return 'bg-yellow-500';
    case 'orange': return 'bg-orange-500';
    case 'red': return 'bg-red-500';
    case 'gray':
    default: return 'bg-gray-500';
  }
}
