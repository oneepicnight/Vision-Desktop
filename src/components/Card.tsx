import type { ReactNode } from "react";

type CardProps = {
  title: string;
  icon: ReactNode;
  children: ReactNode;
};

export function Card({ title, icon, children }: CardProps) {
  return (
    <section className="panel">
      <div className="panel-title">
        {icon}
        <h2>{title}</h2>
      </div>
      {children}
    </section>
  );
}
