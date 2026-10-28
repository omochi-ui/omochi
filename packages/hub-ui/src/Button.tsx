import type { ComponentProps } from 'react';

type ButtonProps = ComponentProps<'button'> & { static?: boolean };

export function Button({ static: isStatic = false, className = '', type = 'button', ...props }: ButtonProps) {
  return <button {...props} type={type} className={`hub-button ${className}`} data-static={isStatic || undefined} />;
}
