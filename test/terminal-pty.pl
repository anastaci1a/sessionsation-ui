use strict;
use warnings;
use FindBin;
use IO::Pty;
use IO::Select;
use Time::HiRes qw(time sleep);
use File::Temp qw(tempdir);
use POSIX qw(WNOHANG);

$ENV{LC_ALL}='C';$ENV{TERM}='xterm-256color';
my $root=tempdir('sessionsation-ui-pty-XXXXXX',TMPDIR=>1,CLEANUP=>1);
mkdir "$root/child dir" or die $!;
open my $backend,'>',"$root/backend" or die $!;
print $backend <<'JS';
#!/usr/local/bin/node
const fs=require('node:fs');
const args=process.argv.slice(7);
fs.appendFileSync(__filename+'.calls',JSON.stringify({args,cwd:process.cwd()})+'\n');
const result=args.join(' ')==='account list'?[{id:'codex:disabled',selectable:false},{id:'codex:pro',selectable:true,plan:'pro'}]:{action:'exec',credentials_in_client:false,argv:[process.execPath,'-e',"console.log('INERT_HANDOFF_NO_PROVIDER '+process.cwd())"],cwd:null};
if(process.env.TEST_SLOW_READ==='1'&&args[0]==='account')setTimeout(()=>console.log(JSON.stringify({api_version:1,ok:true,result})),10000);
else console.log(JSON.stringify({api_version:1,ok:true,result}));
JS
close $backend;chmod 0700,"$root/backend";
$ENV{SESSIONSATION_BIN}="$root/backend";$ENV{SESSIONSATION_DATA_DIR}="$root/data";
$ENV{UI_TEST_ROOT}=$root;$ENV{UI_TEST_MAIN}="$FindBin::Bin/../src/main.mjs";
my ($pty,$pid,$buf,$select);
my $started;
sub start {
  my ($slow)=@_;$ENV{TEST_SLOW_READ}=$slow;
  unlink "$root/backend.calls";
  $pty=IO::Pty->new;$pty->set_winsize(32,100,0,0);
  $started=time;$pid=fork();die $! unless defined $pid;
  if(!$pid){
    $pty->make_slave_controlling_terminal;my $slave=$pty->slave;
    open STDIN,'<&',$slave or die $!;open STDOUT,'>&',$slave or die $!;open STDERR,'>&',$slave or die $!;
    close $pty;chdir $root or die $!;
    exec 'bash','--noprofile','--norc','-c','before=$(stty -g); node "$UI_TEST_MAIN" launcher; rc=$?; after=$(stty -g); if [[ $before == "$after" ]]; then echo TTY_RESTORED; else echo TTY_BROKEN; fi; echo UI_DONE=$rc';die $!;
  }
  $pty->close_slave;$select=IO::Select->new($pty);$buf='';
}
sub pump {
  my ($seconds)=@_;my $end=time+$seconds;
  while(time<$end){for my $fh($select->can_read(0.01)){my $n=sysread($fh,my $part,65536);return unless $n;$buf.=$part;}}
}
sub expect {
  my ($regex,$label)=@_;my $end=time+4;
  while($buf!~$regex&&time<$end){pump(.02);}
  die "FAIL $label (captured ".length($buf)." bytes)\n" unless $buf=~$regex;
}
sub send_keys {syswrite($pty,$_[0]);pump(.08);}
sub finish {
  expect(qr/TTY_RESTORED/,'terminal restored');expect(qr/UI_DONE=0/,'clean exit');
  my $end=time+2;while(waitpid($pid,WNOHANG)==0){die 'hung after exit' if time>$end;sleep .01;}
  die "child failed $?" if $?;close $pty;
}
sub calls {return '' unless -e "$root/backend.calls";open my $f,'<',"$root/backend.calls" or die $!;local $/;return <$f>;}

start(0);expect(qr/\?2004h/,'raw animated screen');
my $startup_ms=int((time-$started)*1000);
if($ENV{UI_TEST_CAPTURE}){pump(.04);open my $capture,'>',$ENV{UI_TEST_CAPTURE} or die $!;print $capture $buf;close $capture;}
my $t=time;syswrite($pty,"\e");finish();my $escape_ms=int((time-$t)*1000);die 'Escape delayed' if time-$t>.4;
die 'home cancel called backend' if calls() ne '';
print "PASS standalone Escape restores terminal without backend calls (startup=${startup_ms}ms cancel=${escape_ms}ms)\n";

start(0);expect(qr/sessionsation/,'home');
$pty->set_winsize(16,44,0,0);kill 'WINCH',$pid;pump(.12);
$pty->set_winsize(40,120,0,0);kill 'WINCH',$pid;pump(.12);
# Fragmented CSI followed by SS3 arrows: both select the inert shell plan.
syswrite($pty,"\e[");pump(.01);send_keys('B');send_keys("\eOB\r");
expect(qr/INERT_HANDOFF_NO_PROVIDER/,'plain shell handoff');finish();
die 'wrong shell request' unless calls()=~/"args":\["launcher","shell"\]/;
die 'missing terminal-bottom repair after resize' unless $buf=~/\e\[40;/;
print "PASS fragmented/SS3 arrows, resize, plain-shell plan and raw cleanup\n";

start(0);expect(qr/sessionsation/,'home');
# Typeahead traverses New -> picker -> child -> account without losing keys.
send_keys("\r\e[B\e[B\e[B\e[C");pump(.15);
send_keys("\e[H\r");pump(.15);send_keys("\r");
expect(qr/INERT_HANDOFF_NO_PROVIDER/,'exact account handoff');finish();
die 'wrong selected account' unless calls()=~/"args":\["launcher","new","--account","codex:pro"\]/;
die 'chosen directory was not inherited' unless $buf=~/INERT_HANDOFF_NO_PROVIDER \Q$root\E\/child dir/;
print "PASS directory browsing, queued keys, disabled-account skip and exact opaque plan\n";

start(1);expect(qr/sessionsation/,'home');send_keys("\r\r");
my $end=time+3;while(calls()!~/account/&&time<$end){pump(.02);}
die 'slow fixture not reached' unless calls()=~/account/;
$t=time;send_keys("\e");send_keys("\e");finish();
die 'pending read blocked cancel' if time-$t>.5;
die 'cancelled read launched a plan' if calls()=~/launcher/;
print "PASS cancel pending read without waiting or launching\n";

start(0);expect(qr/sessionsation/,'home');send_keys("\3");finish();
print "PASS Ctrl-C cleanup\n";
