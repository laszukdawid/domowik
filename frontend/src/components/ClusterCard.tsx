import type { Cluster } from '../types';

interface ClusterCardProps {
  cluster: Cluster;
  onClick: () => void;
}

export default function ClusterCard({ cluster, onClick }: ClusterCardProps) {
  const { stats } = cluster;

  const formatPrice = (price: number) => {
    if (price >= 1000000) {
      return `$${(price / 1000000).toFixed(1)}M`;
    }
    return `$${(price / 1000).toFixed(0)}K`;
  };

  const bedsRange = stats.beds_min && stats.beds_max
    ? stats.beds_min === stats.beds_max
      ? `${stats.beds_min} bd`
      : `${stats.beds_min}-${stats.beds_max} bd`
    : null;

  return (
    <div
      onClick={onClick}
      className="p-3 rounded-lg border cursor-pointer transition-colors border-gray-200 hover:border-gray-300 bg-white hover:bg-gray-50"
    >
      <div className="flex items-center justify-between mb-1">
        <span className="font-medium text-gray-900">{cluster.label}</span>
        <span className="text-sm font-semibold text-blue-600">
          {cluster.count} listings
        </span>
      </div>

      <div className="flex items-center gap-3 text-sm text-gray-600">
        <span>{formatPrice(stats.price_min)} - {formatPrice(stats.price_max)}</span>
        {bedsRange && <span>{bedsRange}</span>}
        {stats.amenity_avg && (
          <span className="text-green-600">Score: {stats.amenity_avg}</span>
        )}
      </div>
    </div>
  );
}
