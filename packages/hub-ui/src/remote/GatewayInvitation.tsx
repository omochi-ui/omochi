import type { TranslationMessage } from '../i18n.js';
import { useTranslation } from 'react-i18next';
import { useEffect, useId, useRef, useState } from 'react';
import { Button } from '../Button.js';
import { parseGatewayLink, type GatewayInvitation as Invitation } from './gateway-link.js';
import type { RemoteHubSession } from './session.js';
interface Job {
  id: string;
  status: string;
  gatewayId: string;
}
interface ScopedJob extends Job {
  scope: string;
  hubName: string;
}
export function GatewayInvitation({
  subscribeInvitation,
  session,
  ownerScope,
  hubName,
  ownsProfile,
}: {
  subscribeInvitation(receive: (fragment: string) => void): () => void;
  session: RemoteHubSession | null;
  /** Hub ID, owner grant ID and authenticated browser identity; stable across reconnects. */
  ownerScope: string | null;
  hubName: string;
  ownsProfile: boolean;
}) {
  const { t, i18n } = useTranslation();
  const [invite, setInvite] = useState<Invitation | null>(null),
    [name, setName] = useState(''),
    [message, setMessage] = useState<TranslationMessage | null>(null),
    [job, setJob] = useState<ScopedJob | null>(null),
    [busy, setBusy] = useState(false);
  const pending = useRef<Invitation | null>(null),
    channel = useRef<BroadcastChannel | null>(null);
  const owner = useRef(ownsProfile);
  owner.current = ownsProfile;
  const [sendAttempt, setSendAttempt] = useState(0);
  const [now, setNow] = useState(Date.now());
  const nameErrorId = useId();
  const nameTooLong = new TextEncoder().encode(name).byteLength > 256;
  const operation = useRef(0);
  useEffect(() => () => { operation.current++; }, []);
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    const receive = (fragment: string) => {
      try {
        if (pending.current) {
          setMessage({ key: 'gateway.busy' });
          return;
        }
        const parsed = parseGatewayLink(fragment);
        pending.current = parsed;
        setInvite(parsed);
        setName(i18n.t('gateway.defaultName', { id: parsed.gatewayId.slice(0, 8) }));
        setJob(null);
        setMessage(null);
      } catch {
        setMessage({ key: 'gateway.invalid' });
      }
    };
    const unsubscribe = subscribeInvitation(receive);
    let bc: BroadcastChannel | null = null;
    try {
      bc = new BroadcastChannel('omochi.gateway-invitation.v1');
      channel.current = bc;
      bc.onmessage = (event) => {
        const v = event.data;
        if (
          v?.kind === 'invitation' &&
          owner.current &&
          typeof v.fragment === 'string' &&
          typeof v.id === 'string'
        ) {
          if (pending.current) {
            const same = '#g:' + pending.current.invitation.slice('omochi://invite/'.length) === v.fragment;
            bc?.postMessage({ kind: same ? 'received' : 'busy', id: v.id });
            return;
          }
          try {
            parseGatewayLink(v.fragment);
            receive(v.fragment);
            bc?.postMessage({ kind: 'received', id: v.id });
          } catch {
            /* Reject malformed data. */
          }
        }
      };
    } catch {
      /* Transfer can be retried when this browser supports channels. */
    }
    return () => {
      unsubscribe();
      bc?.close();
      channel.current = null;
    };
  }, [subscribeInvitation, i18n]);
  useEffect(() => {
    if (!invite || job || ownsProfile || !channel.current) return;
    const bc = channel.current,
      id = crypto.randomUUID();
    let received = false;
    const reply = (event: MessageEvent) => {
      if (event.data?.id !== id) return;
      if (event.data.kind === 'received') {
        received = true;
        pending.current = null;
        setInvite(null);
        setMessage({ key: 'gateway.sent' });
      } else if (event.data.kind === 'busy')
        setMessage({ key: 'gateway.tabBusy' });
    };
    bc.addEventListener('message', reply);
    bc.postMessage({
      kind: 'invitation',
      id,
      fragment: '#g:' + invite.invitation.slice('omochi://invite/'.length),
    });
    const timer = setTimeout(() => {
      if (!received)
        setMessage({ key: 'gateway.connectOrSend' });
    }, 1500);
    return () => {
      clearTimeout(timer);
      bc.removeEventListener('message', reply);
    };
  }, [invite, job, ownsProfile, sendAttempt]);
  useEffect(() => {
    if (!job || job.status !== 'pairing' || !session || ownerScope !== job.scope) return;
    let stopped = false;
    const attempt = operation.current;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const next = (await session.control({ method: 'gatewayPairStatus', id: job.id })) as Job;
        if (!stopped && operation.current === attempt) {
          setJob({ ...next, scope: job.scope, hubName: job.hubName });
          setMessage(null);
        }
      } catch {
        if (!stopped && operation.current === attempt) {
          setMessage({ key: 'gateway.statusError' });
          timer = setTimeout(() => { void poll(); }, 1000);
        }
      }
    };
    // Each response schedules the next render/poll; requests never overlap.
    timer = setTimeout(() => { void poll(); }, 1000);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [job, session, ownerScope]);
  const submit = async () => {
    if (!session || !ownerScope || !invite || busy || !name.trim() || nameTooLong || (job && job.scope !== ownerScope)) return;
    if (job && job.status !== 'savePending' && job.status !== 'failed') return;
    const attempt = ++operation.current;
    const started: ScopedJob = job?.status === 'savePending' ? job : {
      id: '', status: 'starting', gatewayId: invite.gatewayId, scope: ownerScope, hubName,
    };
    setJob(started);
    setBusy(true);
    setMessage(null);
    try {
      const result = (await session.control(
        started.status === 'savePending'
          ? { method: 'gatewayPairRetrySave', id: started.id }
          : { method: 'gatewayPairStart', invitation: invite.invitation, name },
      )) as Job;
      // Dismissal or a new invitation invalidates old results. A reconnect may
      // retain the result, but always under the original authority and hub name.
      if (operation.current === attempt) setJob({ ...result, scope: started.scope, hubName: started.hubName });
    } catch {
      if (operation.current === attempt) {
        setJob({ ...started, status: started.status === 'savePending' ? 'savePending' : 'outcomeUnknown' });
        setMessage(started.status === 'savePending' ? { key: 'gateway.statusError' } : null);
      }
    } finally {
      if (operation.current === attempt) setBusy(false);
    }
  };
  if (!invite) return message ? <p role="status">{t(message.key, { ...message.values })}</p> : null;
  return (
    <section className="gateway-invitation" aria-label={t('gateway.add')}>
      <h2>{t('gateway.title')}</h2>
      <p>
        {t('gateway.id')} <code>{invite.gatewayId}</code>
      </p>
      <p>{t('gateway.expires', { date: new Date(invite.expires * 1000).toLocaleString(i18n.resolvedLanguage) })}</p>
      <p>
        {job || session
          ? t('gateway.hub', { name: job?.hubName ?? hubName })
          : t('gateway.connectHelp')}
      </p>
      <label>
        {t('gateway.name')}{' '}
        <input
          value={name}
          maxLength={256}
          aria-invalid={nameTooLong || undefined}
          aria-describedby={nameTooLong ? nameErrorId : undefined}
          disabled={busy || (!!job && job.status !== 'failed')}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      {nameTooLong && <p id={nameErrorId} role="status">{t('gateway.nameTooLong')}</p>}
      <p role="status">
        {invite.expires * 1000 <= now && !job
          ? t('gateway.expired')
          : job?.status === 'registered'
            ? t('gateway.registered')
            : job?.status === 'savePending'
              ? t('gateway.savePending')
              : job?.status === 'outcomeUnknown'
                ? t('gateway.unknown')
                : job?.status === 'failed'
                  ? t('gateway.failed')
                  : job?.status === 'pairing' || job?.status === 'starting'
                    ? t('gateway.registering')
                    : message && t(message.key, { ...message.values })}
      </p>
      {job && job.scope !== ownerScope && <p role="status">{t('gateway.reconnect', { name: job.hubName })}</p>}
      {job && message && <p role="status">{t(message.key, { ...message.values })}</p>}
      {job && !['registered', 'failed'].includes(job.status) && <p>{t('gateway.dismissHelp')}</p>}
      <Button
        disabled={
          !session ||
          !ownerScope ||
          (job && ownerScope !== job.scope) ||
          (invite.expires * 1000 <= now && job?.status !== 'savePending') ||
          !name.trim() ||
          nameTooLong ||
          busy ||
          (!!job && job.status !== 'savePending' && job.status !== 'failed')
        }
        onClick={() => {
          void submit();
        }}
      >
        {job?.status === 'savePending' ? t('gateway.retrySave') : t('gateway.confirm')}
      </Button>
      <a className="hub-button" href={invite.invitation}>Open in Omochi app</a>
      {!ownsProfile && !job && (
        <Button onClick={() => setSendAttempt((v) => v + 1)}>{t('gateway.send')}</Button>
      )}
      <Button
        onClick={() => {
          operation.current++;
          setBusy(false);
          pending.current = null;
          setInvite(null);
          setJob(null);
          setMessage(null);
        }}
      >
        {t('gateway.dismiss')}
      </Button>
    </section>
  );
}
