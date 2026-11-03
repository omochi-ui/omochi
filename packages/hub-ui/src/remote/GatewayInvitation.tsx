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
  const [invite, setInvite] = useState<Invitation | null>(null),
    [name, setName] = useState(''),
    [message, setMessage] = useState(''),
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
          setMessage('Another gateway invitation is awaiting confirmation.');
          return;
        }
        const parsed = parseGatewayLink(fragment);
        pending.current = parsed;
        setInvite(parsed);
        setName(`Gateway ${parsed.gatewayId.slice(0, 8)}`);
        setJob(null);
        setMessage('');
      } catch {
        setMessage('Invalid or expired gateway invitation. Request a new invitation.');
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
  }, [takeInvitation]);
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
        setMessage(
          'Invitation sent. Confirm the gateway in the tab already connected to your hub.',
        );
      } else if (event.data.kind === 'busy')
        setMessage(
          'The connected tab already has an invitation. Finish or dismiss it, then retry sending.',
        );
    };
    bc.addEventListener('message', reply);
    bc.postMessage({
      kind: 'invitation',
      id,
      fragment: '#g:' + invite.invitation.slice('omochi://invite/'.length),
    });
    const timer = setTimeout(() => {
      if (!received)
        setMessage('Connect an owner hub here, or retry sending to the connected tab.');
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
          setMessage('Registration status unavailable. Reconnect this hub to check again.');
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
    setMessage('');
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
      setMessage(
        'Could not start registration. Check the owner connection, invitation expiry and whether another registration is running.',
      );
    } finally {
      setBusy(false);
    }
  };
  if (!invite) return message ? <p role="status">{message}</p> : null;
  return (
    <section className="gateway-invitation" aria-label="Add gateway">
      <h2>Add gateway to hub</h2>
      <p>
        Gateway: <code>{invite.gatewayId}</code>
      </p>
      <p>Expires: {new Date(invite.expires * 1000).toLocaleString()}</p>
      <p>
        {session
          ? `Hub: ${hubName}`
          : 'Connect an owner hub below to add this gateway. If no owner connection is saved, open an owner invitation from your hub first.'}
      </p>
      <label>
        Gateway name{' '}
        <input
          value={name}
          maxLength={256}
          disabled={busy || job?.status === 'pairing'}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      <p role="status">
        {invite.expires * 1000 <= now && !job
          ? 'Invitation expired. Request a new invitation.'
          : job?.status === 'registered'
            ? 'Gateway registered. Apps will appear when the gateway connects.'
            : job?.status === 'savePending'
              ? 'Gateway accepted registration, but saving on the hub failed. Retry saving.'
              : job?.status === 'outcomeUnknown'
                ? 'Registration outcome unknown. Retry with this same hub while the invitation is valid, or request a new invitation.'
                : job?.status === 'failed'
                  ? 'Registration failed. Check the invitation and retry.'
                  : job?.status === 'pairing'
                    ? 'Registering gateway…'
                    : message}
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
        {job?.status === 'savePending' ? 'Retry saving registration' : 'Add to this hub'}
      </Button>
      <a className="hub-button" href={invite.invitation}>Open in Omochi app</a>
      {!ownsProfile && (
        <Button onClick={() => setSendAttempt((v) => v + 1)}>Send to connected tab</Button>
      )}
      <Button
        disabled={busy || job?.status === 'pairing'}
        onClick={() => {
          pending.current = null;
          setInvite(null);
          setJob(null);
          setMessage('');
        }}
      >
        Dismiss invitation
      </Button>
    </section>
  );
}
