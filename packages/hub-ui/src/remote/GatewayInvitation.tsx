import type { TranslationMessage } from '../i18n.js';
import { useTranslation } from 'react-i18next';
import { useEffect, useRef, useState } from 'react';
import { Button } from '../Button.js';
import { parseGatewayLink, type GatewayInvitation as Invitation } from './gateway-link.js';
import type { RemoteHubSession } from './session.js';
interface Job {
  id: string;
  status: string;
  gatewayId: string;
}
export function GatewayInvitation({
  takeInvitation,
  session,
  hubName,
  ownsProfile,
}: {
  takeInvitation(): string | null;
  session: RemoteHubSession | null;
  hubName: string;
  ownsProfile: boolean;
}) {
  const { t, i18n } = useTranslation();
  const [invite, setInvite] = useState<Invitation | null>(null),
    [name, setName] = useState(''),
    [message, setMessage] = useState<TranslationMessage | null>(null),
    [job, setJob] = useState<Job | null>(null),
    [busy, setBusy] = useState(false);
  const pending = useRef<Invitation | null>(null),
    channel = useRef<BroadcastChannel | null>(null);
  const owner = useRef(ownsProfile);
  owner.current = ownsProfile;
  const [sendAttempt, setSendAttempt] = useState(0);
  const [now, setNow] = useState(Date.now());
  const jobSession = useRef<RemoteHubSession | null>(null);
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
    const initial = takeInvitation();
    if (initial) receive(initial);
    const hash = () => {
      const fragment = location.hash;
      if (fragment.startsWith('#g:')) {
        history.replaceState(null, '', location.pathname + location.search);
        receive(fragment);
      }
    };
    window.addEventListener('hashchange', hash);
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
      window.removeEventListener('hashchange', hash);
      bc?.close();
      channel.current = null;
    };
  }, [takeInvitation, i18n]);
  useEffect(() => {
    if (!invite || ownsProfile || !channel.current) return;
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
  }, [invite, ownsProfile, sendAttempt]);
  useEffect(() => {
    if (!job || job.status !== 'pairing' || !session || session !== jobSession.current) return;
    let stopped = false;
    const poll = async () => {
      try {
        const next = (await session.control({ method: 'gatewayPairStatus', id: job.id })) as Job;
        if (!stopped) setJob(next);
      } catch {
        if (!stopped)
          setMessage({ key: 'gateway.statusError' });
      }
    };
    const timer = setInterval(() => {
      void poll();
    }, 1000);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  }, [job, session]);
  const submit = async () => {
    if (!session || !invite || busy) return;
    setBusy(true);
    setMessage(null);
    jobSession.current = session;
    try {
      setJob(
        (await session.control(
          job?.status === 'savePending'
            ? { method: 'gatewayPairRetrySave', id: job.id }
            : { method: 'gatewayPairStart', invitation: invite.invitation, name },
        )) as Job,
      );
    } catch {
      setMessage({ key: 'gateway.startError' });
    } finally {
      setBusy(false);
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
        {session
          ? t('gateway.hub', { name: hubName })
          : t('gateway.connectHelp')}
      </p>
      <label>
        {t('gateway.name')}{' '}
        <input
          value={name}
          maxLength={256}
          disabled={busy || job?.status === 'pairing'}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
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
                  : job?.status === 'pairing'
                    ? t('gateway.registering')
                    : message && t(message.key, { ...message.values })}
      </p>
      <Button
        disabled={
          !session ||
          (job && session !== jobSession.current) ||
          (invite.expires * 1000 <= now && job?.status !== 'savePending') ||
          !name.trim() ||
          busy ||
          job?.status === 'pairing' ||
          job?.status === 'registered'
        }
        onClick={() => {
          void submit();
        }}
      >
        {job?.status === 'savePending' ? t('gateway.retrySave') : t('gateway.confirm')}
      </Button>
      {!ownsProfile && (
        <Button onClick={() => setSendAttempt((v) => v + 1)}>{t('gateway.send')}</Button>
      )}
      <Button
        disabled={busy || job?.status === 'pairing'}
        onClick={() => {
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
